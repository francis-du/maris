//! Independent desktop status process. Closing a TUI does not silently stop background audio.
pub mod controls;
#[cfg(all(feature = "desktop", target_os = "macos"))]
mod menu_capture;
#[cfg(all(feature = "desktop", target_os = "macos"))]
mod menu_header;
#[cfg(all(target_os = "macos", feature = "desktop"))]
pub(crate) mod monitor;
pub mod monitor_state;
#[cfg(feature = "desktop")]
mod native_mark;
pub mod status_icon;

#[cfg(feature = "desktop")]
use crate::i18n::text as t;
use crate::{
    audio, control,
    control::store::{read_json, Store},
};
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn named_lock(store: &Store, name: &str) -> Result<File> {
    std::fs::create_dir_all(&store.directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(store.directory.join(name))?;
    file.try_lock_exclusive()
        .with_context(|| format!("Another Maris process holds {name}"))?;
    Ok(file)
}
fn detach(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: this pre-exec hook only invokes async-signal-safe setsid and returns errno.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008);
    }
}
fn spawn(store: &Store, args: &[String], log_name: &str) -> Result<Child> {
    std::fs::create_dir_all(&store.directory)?;
    let log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(store.directory.join(log_name))?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--lang", crate::i18n::code()])
        .args(args)
        .env("MARIS_STATE_DIR", &store.directory)
        .env("MARIS_LANG", crate::i18n::code())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    detach(&mut command);
    Ok(command.spawn()?)
}
fn detail(store: &Store, log: &str) -> String {
    std::fs::read_to_string(store.directory.join(log))
        .unwrap_or_default()
        .chars()
        .take(1000)
        .collect()
}

pub fn ensure_running(store: &Store) -> Result<()> {
    ensure!(
        cfg!(feature = "desktop"),
        "Desktop support was not compiled; build with --features desktop or use --no-tray"
    );
    if named_lock(store, "desktop.lock").is_err() {
        return Ok(());
    }
    let mut child = spawn(store, &["tray".into()], "desktop.log")?;
    let start = Instant::now();
    loop {
        if let Ok(state) = read_json::<Value>(&store.directory.join("desktop.json")) {
            if state["pid"].as_u64() == Some(child.id() as u64) && state["active"] == true {
                return Ok(());
            }
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "Desktop indicator exited ({status}). {}",
                detail(store, "desktop.log")
            );
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Desktop indicator did not become ready. Read desktop.log or use --no-tray");
        }
        thread::sleep(Duration::from_millis(40));
    }
}

pub fn launch_audio(store: &Store, args: &[String]) -> Result<Value> {
    ensure!(
        matches!(
            args.first().map(String::as_str),
            Some("run" | "play" | "system" | "application" | "voice")
        ),
        "Unsupported background audio command"
    );
    ensure!(
        control::is_stopped(store),
        "Audio is already running. Stop it before selecting another source"
    );
    let mut command = vec!["--no-tray".to_owned()];
    command.extend_from_slice(args);
    let mut child = spawn(store, &command, "audio.log")?;
    let start = Instant::now();
    loop {
        let state = audio::runtime_status(store);
        if state["pid"].as_u64() == Some(child.id() as u64) && state["active"] == true {
            return Ok(state);
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "Audio did not start ({status}). {}",
                detail(store, "audio.log")
            );
        }
        if start.elapsed() > Duration::from_secs(8) {
            // Never kill a process that might own a route. Retain its PID in an explicit pending record.
            store.write_json(
                "pending-audio.json",
                &json!({"pid":child.id(),"started_at_ms":crate::analysis::now_ms()}),
            )?;
            return Ok(
                json!({"active":false,"pending":true,"pid":child.id(),"phase":"awaiting_os_authorization"}),
            );
        }
        thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(feature = "desktop")]
mod events;

#[cfg(not(feature = "desktop"))]
pub fn run(_store: Store) -> Result<()> {
    bail!("Desktop support is not compiled; use --features desktop")
}

#[cfg(feature = "desktop")]
pub fn run(store: Store) -> Result<()> {
    let _lease = named_lock(&store, "desktop.lock")?;
    let quitting = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = quitting.clone();
    ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::Relaxed))?;
    native::event_loop(store, quitting, false)
}

/// Native application entry: the tray is present before requesting audio permission.
#[cfg(feature = "desktop")]
pub fn run_app(store: Store) -> Result<()> {
    if named_lock(&store, "desktop.lock").is_err() {
        if control::is_stopped(&store) {
            launch_audio(&store, &["system".into(), "--accept-routing".into()])?;
        }
        return Ok(());
    }
    let _lease = named_lock(&store, "desktop.lock")?;
    let quitting = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = quitting.clone();
    ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::Relaxed))?;
    native::event_loop(store, quitting, true)
}
#[cfg(not(feature = "desktop"))]
pub fn run_app(_store: Store) -> Result<()> {
    bail!("Desktop support is not compiled; use the explicit system command")
}

pub fn package_macos() -> Result<Value> {
    ensure!(
        cfg!(target_os = "macos"),
        "Application bundle packaging is macOS-only"
    );
    let bundle = std::env::current_dir()?.join("dist/Maris.app");
    let directory = bundle.join("Contents/MacOS");
    std::fs::create_dir_all(&directory)?;
    // Replace the bundle executable atomically; a running process keeps its old inode.
    let temporary = tempfile::NamedTempFile::new_in(&directory)?;
    std::fs::copy(std::env::current_exe()?, temporary.path())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(directory.join("maris"))
        .map_err(|error| error.error)?;
    std::fs::write(
        bundle.join("Contents/Info.plist"),
        include_str!("../../../Maris-Info.plist"),
    )?;
    Ok(
        json!({"application":bundle,"signed":false,"notarized":false,"launch":"Open Maris.app and allow system audio recording"}),
    )
}

/// Inspect native items and their actual action dispatcher in an isolated offline fixture.
/// The confirmation response is injected; no live capture or user menu event is consumed.
#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "windows")))]
pub fn menu_review() -> Result<Value> {
    native::menu_review(false)
}

/// Explicit offline AppKit captures; ordinary native construction checks never pop up menus.
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub fn menu_capture_review() -> Result<Value> {
    native::menu_review(true)
}

#[cfg(feature = "desktop")]
mod native;
