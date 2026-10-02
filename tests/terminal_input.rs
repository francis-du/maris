//! Exercise the actual TUI process using an owned pseudo-terminal and isolated offline state.
//! A parent-held session lease prevents the console from starting an audio engine.
#![cfg(unix)]
use maris::{configuration, control_panel, listening, store::Store, studio_controls};
use ratatui::layout::Rect;
use serde_json::json;
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct ConsoleProcess {
    child: Child,
    master: File,
    first_output: String,
}

/// Fixture status must continue independently while the terminal blocks on a burst.
/// This does not weaken the production timeout or grant a real capture session.
struct Heartbeat {
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Heartbeat {
    fn start(store: Store) -> Self {
        heartbeat(&store);
        let (stop, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(Duration::from_millis(100)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                heartbeat(&store);
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ConsoleProcess {
    fn drop(&mut self) {
        if let Err(error) = self.kill_and_reap() {
            if std::thread::panicking() {
                eprintln!("Offline console cleanup failed: {error}");
            } else {
                panic!("Offline console cleanup failed: {error}");
            }
        }
    }
}
impl ConsoleProcess {
    fn kill_and_reap(&mut self) -> std::io::Result<()> {
        // This child is exclusively owned by this offline test, not the user's Maris process.
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        // On macOS, terminal teardown can still need its output drained after
        // SIGKILL. A blocking wait here deadlocked all parallel preset tests.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut discarded = String::new();
        while self.child.try_wait()?.is_none() {
            self.drain(&mut discarded);
            discarded.clear();
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Owned pseudo-terminal child was not reaped",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
    fn drain(&mut self, output: &mut String) {
        let mut bytes = [0_u8; 8192];
        // A continuously readable terminal must not starve the caller's deadline.
        for _ in 0..16 {
            match self.master.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    let text = String::from_utf8_lossy(&bytes[..count]);
                    output.push_str(&text);
                    // Retain early notices even when send() drains output to avoid PTY deadlock.
                    if self.first_output.len() < 65_536 {
                        self.first_output.push_str(&text);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
    fn send(&mut self, input: &str) {
        let mut data = input.as_bytes();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut discarded = String::new();
        while !data.is_empty() {
            self.drain(&mut discarded);
            discarded.clear();
            match self.master.write(data) {
                Ok(count) => data = &data[count..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("Pseudo-terminal write failed: {e}"),
            }
            assert!(Instant::now() < deadline, "Pseudo-terminal input stalled");
        }
    }
}

fn heartbeat(store: &Store) {
    store.write_json("runtime.json", &json!({
        "active":true,"session_id":"offline-pty-fixture","profile_key":"OFFLINE PTY fixture",
        "output":"OFFLINE PTY fixture","sample_rate":48000,"music_processing":true,
        "updated_at_ms":maris::analysis::now_ms(),"rebind_count":0,"device_binding_revision":0,
        "device_identity":{"stable_id":"offline-pty"},"output_mode":"pinned",
        "applied_revision":0,"applied_music_revision":0
    })).unwrap();
}
fn click(rect: Rect) -> String {
    let x = rect.x + rect.width / 2 + 1;
    let y = rect.y + rect.height / 2 + 1;
    format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m")
}

fn spawn_console(dir: &std::path::Path) -> ConsoleProcess {
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 40,
        ws_col: 140,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: writable descriptor outputs and a live winsize; no custom termios pointer.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::addr_of_mut!(size),
            )
        },
        0
    );
    // SAFETY: openpty returned two newly owned, distinct valid file descriptors.
    let master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        // SAFETY: both descriptors are live and owned here. Do not leak terminal
        // endpoints into the console or another concurrently exec'd test child.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) },
            0
        );
    }
    // SAFETY: fcntl changes only the owned master descriptor's status flags.
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_maris"));
    command
        .args(["--no-tray", "--lang", "en", "tui"])
        .env("MARIS_STATE_DIR", dir)
        .env("TERM", "xterm-256color")
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave))
        .stderr(Stdio::null());
    // SAFETY: only setsid/ioctl/errno operations run after fork. fd 0 is the owned slave.
    // crossterm opens /dev/tty; without a private controlling terminal it can observe
    // the caller's terminal rather than the test's declared dimensions and input.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    drop(command);
    let mut process = ConsoleProcess {
        child,
        master,
        first_output: String::new(),
    };
    let start = Instant::now();
    let mut output = String::new();
    while !output.contains("MARIS") {
        process.drain(&mut output);
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "Offline console exited before drawing"
        );
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Console did not draw"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    process
}

#[test]
fn real_terminal_mouse_bursts_reach_draft_and_apply_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    let mut output = String::new();
    process.send("e");
    output.clear();
    let start = Instant::now();
    while !output.contains("Current") {
        process.drain(&mut output);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "Settings editor did not open"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let areas = configuration::layout(studio_controls::shell(Rect::new(0, 0, 140, 40)).content);
    for step in 0..16 {
        // Motion was previously rendered one event at a time, burying real button events.
        process.send(&format!(
            "{}{}",
            "\x1b[<35;110;20M".repeat(160),
            click(if step < 12 { areas.plus } else { areas.minus })
        ));
    }
    process.send(&click(areas.apply));
    let start = Instant::now();
    while store.load().unwrap().revision == 0 {
        process.drain(&mut output);
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "Console exited during click handling"
        );
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Button burst did not commit its draft; first output: {}\nterminal tail: {}",
            process.first_output,
            output
                .chars()
                .rev()
                .take(6000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let saved = store.load().unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(
        saved.profile.bands[0].gain_db,
        4.0,
        "A physical +/- click was lost or duplicated; first output: {}\nterminal tail: {}",
        process.first_output,
        output
            .chars()
            .rev()
            .take(6000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());

    // Human-paced edits cross the live telemetry window. Keep the engine fixture
    // alive while the user reads the draft; Enter must still commit that same draft.
    process.send("+");
    let editing = Instant::now();
    while editing.elapsed() < Duration::from_millis(1400) {
        process.drain(&mut output);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        store.load().unwrap().revision,
        1,
        "draft wrote before Enter"
    );
    process.send("\r");
    let start = Instant::now();
    while store.load().unwrap().revision < 2 {
        process.drain(&mut output);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "delayed Enter was lost"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 4.5);

    // Legacy terminals emit only Press events. An intervening '+' makes this
    // Enter deliberate even inside the discrete-key debounce interval.
    process.send("+\r");
    let start = Instant::now();
    while store.load().unwrap().revision < 3 {
        process.drain(&mut output);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "rapid new Enter was lost"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(store.load().unwrap().revision, 3);
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 5.0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
    process.send("q");
    let start = Instant::now();
    while process.child.try_wait().unwrap().is_none() {
        process.drain(&mut output);
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(control_panel::EQ_ROW_START, 16);
}

#[test]
fn real_terminal_child_with_unread_output_is_reaped_before_fixture_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p");
    // Deliberately leave the popup frame unread, as a real early-exit test can.
    std::thread::sleep(Duration::from_millis(150));
    let start = Instant::now();
    process.kill_and_reap().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(process.child.try_wait().unwrap().is_some());
    // Explicit cleanup and Drop must not attempt to reap or signal a reused PID.
    process.kill_and_reap().unwrap();
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
}

#[path = "unit/terminal_presets.rs"]
mod preset_tests;
