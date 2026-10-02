//! Local PCM transport. Blocking or waiting is confined to this worker, never an audio callback.
use super::server::Server;
use crate::audio::{bridge::Renderer, Metrics, Settings, Update};
use anyhow::{ensure, Context, Result};
use crossbeam_queue::ArrayQueue;
use std::{
    io::{self, Read, Write},
    os::fd::AsRawFd,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub(super) struct Worker {
    children: [OwnedChild; 2],
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    metrics: Arc<Metrics>,
}
fn nonblocking(fd: i32) -> Result<()> {
    // SAFETY: the caller retains a live pipe descriptor throughout this call.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
        "Cannot configure audio pipe: {}",
        io::Error::last_os_error()
    );
    Ok(())
}
impl Worker {
    pub fn start(
        server: &Server,
        monitor: &str,
        output: &str,
        token: &str,
        settings: Settings,
        updates: Arc<ArrayQueue<Update>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let capture = server.stream_command(true, monitor, &format!("{token}_capture"))?;
        let playback = server.stream_command(false, output, token)?;
        Self::spawn(capture, playback, settings, updates, metrics)
    }
    fn spawn(
        mut capture: Command,
        mut playback: Command,
        settings: Settings,
        updates: Arc<ArrayQueue<Update>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let mut playback = OwnedChild(
            playback
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .context("Cannot start local audio playback")?,
        );
        let mut capture = OwnedChild(
            capture
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .context("Cannot start local monitor capture")?,
        );
        let input = capture
            .0
            .stdout
            .take()
            .context("Missing audio capture pipe")?;
        let output = playback
            .0
            .stdin
            .take()
            .context("Missing audio playback pipe")?;
        nonblocking(input.as_raw_fd())?;
        nonblocking(output.as_raw_fd())?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let ready = Arc::new(AtomicBool::new(false));
        let ready_flag = ready.clone();
        let counters = metrics.clone();
        let mut renderer = Renderer::for_worker(settings, 48_000, updates, metrics.clone());
        let thread = thread::Builder::new()
            .name("maris-pulse-pcm".into())
            .spawn(move || {
                if transfer(input, output, &flag, &ready_flag, &counters, &mut renderer).is_err()
                    && !flag.load(Ordering::Acquire)
                {
                    counters.errors.fetch_add(1, Ordering::Relaxed);
                }
            })?;
        Ok(Self {
            children: [capture, playback],
            stop,
            ready,
            thread: Some(thread),
            metrics,
        })
    }
    pub fn arm(&self) {
        self.ready.store(true, Ordering::Release);
    }
    pub fn healthy(&mut self) -> bool {
        self.metrics.errors.load(Ordering::Relaxed) == 0
            && self
                .children
                .iter_mut()
                .all(|child| matches!(child.0.try_wait(), Ok(None)))
            && self
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.metrics.stopping.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_millis(75);
        while !self.metrics.faded_out.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        self.stop.store(true, Ordering::Release);
        // Only the two children started above are terminated. Closing their pipes unblocks transport.
        for child in &mut self.children {
            let _ = child.0.kill();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) struct CaptureWorker {
    child: OwnedChild,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    metrics: Arc<Metrics>,
}
impl CaptureWorker {
    pub fn start(
        server: &Server,
        monitor: &str,
        token: &str,
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let mut command = server.stream_command(true, monitor, token)?;
        let mut child = OwnedChild(
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .context("Cannot start application monitor capture")?,
        );
        let input = child
            .0
            .stdout
            .take()
            .context("Missing application capture pipe")?;
        nonblocking(input.as_raw_fd())?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let counters = metrics.clone();
        let thread = thread::Builder::new()
            .name("maris-pulse-strip".into())
            .spawn(move || {
                if capture_frames(input, &flag, &counters, &queue).is_err()
                    && !flag.load(Ordering::Acquire)
                {
                    counters.errors.fetch_add(1, Ordering::Relaxed);
                }
            })?;
        Ok(Self {
            child,
            stop,
            thread: Some(thread),
            metrics,
        })
    }
    pub fn healthy(&mut self) -> bool {
        self.metrics.errors.load(Ordering::Relaxed) == 0
            && matches!(self.child.0.try_wait(), Ok(None))
            && self
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }
}
impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.child.0.kill();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn wait(stop: &AtomicBool, last_progress: Instant) -> io::Result<()> {
    if stop.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Audio transport stopped",
        ));
    }
    if last_progress.elapsed() > Duration::from_secs(3) {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Local audio transport stalled",
        ));
    }
    thread::sleep(Duration::from_millis(1));
    Ok(())
}
fn capture_frames(
    mut input: impl Read,
    stop: &AtomicBool,
    metrics: &Metrics,
    queue: &ArrayQueue<[f32; 2]>,
) -> io::Result<()> {
    let mut bytes = [0_u8; 480 * 8];
    while !stop.load(Ordering::Acquire) {
        let mut filled = 0;
        let mut progress = Instant::now();
        while filled < bytes.len() {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            match input.read(&mut bytes[filled..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Application capture ended",
                    ))
                }
                Ok(count) => {
                    filled += count;
                    progress = Instant::now();
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => wait(stop, progress)?,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        for frame in bytes.as_chunks::<8>().0 {
            let values = [
                f32::from_le_bytes(frame[..4].try_into().expect("four bytes")),
                f32::from_le_bytes(frame[4..].try_into().expect("four bytes")),
            ]
            .map(|value| if value.is_finite() { value } else { 0.0 });
            if queue.push(values).is_err() {
                metrics.overruns.fetch_add(1, Ordering::Relaxed);
            }
        }
        metrics.captured_frames.fetch_add(480, Ordering::Relaxed);
    }
    Ok(())
}

fn transfer(
    mut input: impl Read,
    mut output: impl Write,
    stop: &AtomicBool,
    ready: &AtomicBool,
    metrics: &Metrics,
    renderer: &mut Renderer,
) -> io::Result<()> {
    let mut bytes = [0_u8; 480 * 8];
    let mut samples = [0_f32; 480 * 2];
    while !stop.load(Ordering::Acquire) {
        let mut filled = 0;
        let mut progress = Instant::now();
        while filled < bytes.len() {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            match input.read(&mut bytes[filled..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Audio capture ended",
                    ))
                }
                Ok(count) => {
                    filled += count;
                    progress = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => wait(stop, progress)?,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        metrics.captured_frames.fetch_add(480, Ordering::Relaxed);
        let mut frames = bytes.as_chunks::<8>().0.iter();
        let route_ready = ready.load(Ordering::Acquire);
        renderer.render(&mut samples, 2, || {
            let frame = frames.next().expect("one input for each output frame");
            let values = [
                f32::from_le_bytes(frame[..4].try_into().expect("four bytes")),
                f32::from_le_bytes(frame[4..].try_into().expect("four bytes")),
            ];
            (
                values.map(|v| if v.is_finite() { v } else { 0.0 }),
                route_ready,
            )
        });
        for (sample, destination) in samples.iter().zip(bytes.as_chunks_mut::<4>().0) {
            destination.copy_from_slice(&sample.to_le_bytes());
        }
        let mut written = 0;
        progress = Instant::now();
        while written < bytes.len() {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            match output.write(&bytes[written..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Audio playback ended",
                    ))
                }
                Ok(count) => {
                    written += count;
                    progress = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => wait(stop, progress)?,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/pulse_stream.rs"]
mod tests;
