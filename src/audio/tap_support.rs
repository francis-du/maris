use super::{tap_ffi as ca, Metrics};
use anyhow::{ensure, Result};
use crossbeam_queue::ArrayQueue;
use objc2::{runtime::AnyClass, sel};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

pub(super) struct Capture {
    pub(super) queue: Arc<ArrayQueue<[f32; 2]>>,
    pub(super) metrics: Arc<Metrics>,
    pub(super) next_sample: AtomicU64,
    pub(super) gate: AtomicU64,
}

pub(super) const MAX_CAPTURE_FRAMES: usize = 65_536;

// Hold a complete supported callback plus the previous block's playback reserve.
pub(in crate::audio) fn capture_queue() -> Arc<ArrayQueue<[f32; 2]>> {
    Arc::new(ArrayQueue::new(MAX_CAPTURE_FRAMES * 2))
}

pub(super) const CAPTURE_GATE_BLOCKED: u64 = 1;
pub(super) const CAPTURE_GATE_READER: u64 = 2;

pub(super) struct CaptureLease<'a>(&'a AtomicU64);
impl Drop for CaptureLease<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(CAPTURE_GATE_READER, Ordering::Release);
    }
}

pub(super) fn enter_capture(gate: &AtomicU64) -> Option<CaptureLease<'_>> {
    let mut state = gate.load(Ordering::Acquire);
    loop {
        if state & CAPTURE_GATE_BLOCKED != 0 {
            return None;
        }
        let next = state.checked_add(CAPTURE_GATE_READER)?;
        match gate.compare_exchange_weak(state, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Some(CaptureLease(gate)),
            Err(current) => state = current,
        }
    }
}

pub(super) fn block_capture(gate: &AtomicU64, timeout: std::time::Duration) -> Result<()> {
    let previous = gate.fetch_or(CAPTURE_GATE_BLOCKED, Ordering::AcqRel);
    ensure!(
        previous & CAPTURE_GATE_BLOCKED == 0,
        "System-audio capture is already quarantined"
    );
    let deadline = std::time::Instant::now() + timeout;
    while gate.load(Ordering::Acquire) != CAPTURE_GATE_BLOCKED {
        if std::time::Instant::now() >= deadline {
            gate.fetch_and(!CAPTURE_GATE_BLOCKED, Ordering::Release);
            anyhow::bail!("Timed out quarantining system-audio capture");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Ok(())
}

pub(super) fn unblock_capture(gate: &AtomicU64) {
    gate.fetch_and(!CAPTURE_GATE_BLOCKED, Ordering::Release);
}

pub(super) fn quarantine_then_restore(
    metrics: &Metrics,
    restore: impl FnOnce() -> Result<()>,
) -> Result<()> {
    // A replacement callback may resume after the bounded fade wait expires. Block
    // it at both renderer quarantine checks before restoring the original path.
    metrics.output_quarantined.store(true, Ordering::Release);
    restore()
}

pub(super) fn device_list_changed(object: u32, addresses: &[ca::Address]) -> bool {
    object == ca::SYSTEM_OBJECT
        && addresses
            .iter()
            .any(|address| address.selector == u32::from_be_bytes(*b"dev#"))
}

pub(super) unsafe extern "C" fn configuration_event(
    object: u32,
    count: u32,
    addresses: *const ca::Address,
    context: *mut c_void,
) -> i32 {
    if !context.is_null() {
        // Listener state shares the capture's owned lifetime; no native queries,
        // locks, allocations or profile writes occur on this callback.
        let capture = unsafe { &*context.cast::<Capture>() };
        if object == ca::SYSTEM_OBJECT && !addresses.is_null() {
            let addresses = unsafe { std::slice::from_raw_parts(addresses, count as usize) };
            if device_list_changed(object, addresses) {
                capture
                    .metrics
                    .device_list_events
                    .fetch_add(1, Ordering::Release);
            }
        }
        // Publish the cause before its generation so an acquiring observer cannot
        // acknowledge the event without checking the changed device topology.
        capture
            .metrics
            .configuration_events
            .fetch_add(1, Ordering::Release);
    }
    0
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutputConfiguration {
    pub(super) rate: u32,
    pub(super) buffer: Option<u32>,
    pub(super) latency: Option<u32>,
    pub(super) safety: Option<u32>,
    pub(super) alive: Option<u32>,
}
impl OutputConfiguration {
    pub(super) fn read(device: u32) -> Result<Self> {
        Ok(Self {
            rate: ca::sample_rate(device)?,
            buffer: ca::output_buffer_frames(device).ok(),
            latency: ca::output_latency_frames(device).ok(),
            safety: ca::output_safety_offset_frames(device).ok(),
            alive: unsafe { ca::property(device, b"livn") }.ok(),
        })
    }
}

pub(super) fn default_output_changed(expected: Option<u32>, current: Option<u32>) -> bool {
    expected.is_some() && expected != current
}

pub(super) fn validate_process_set(processes: &[u32], available: &[u32], own: u32) -> Result<()> {
    ensure!(
        !processes.is_empty() && processes.len() <= 64,
        "Select between 1 and 64 audio process objects"
    );
    for (index, process) in processes.iter().copied().enumerate() {
        ensure!(
            process != 0 && process != own,
            "Invalid audio process object {process}"
        );
        ensure!(
            !processes[..index].contains(&process),
            "Duplicate audio process object {process}"
        );
        ensure!(
            available.contains(&process),
            "Audio process object {process} is no longer available"
        );
    }
    Ok(())
}

pub(super) fn validate_processes(processes: &[u32]) -> Result<()> {
    let available = ca::process_ids()?;
    let own = ca::own_process()?;
    validate_process_set(processes, &available, own)
}

// Internal aggregate churn does not invalidate the physical audio graph.
pub(super) fn device_topology(
    devices: impl IntoIterator<Item = (u32, Option<String>)>,
) -> Vec<(u32, Option<String>)> {
    let mut devices: Vec<_> = devices
        .into_iter()
        .filter(|(_, uid)| {
            !uid.as_deref()
                .is_some_and(|uid| uid.starts_with("audio.maris.tap."))
        })
        .collect();
    devices.sort_by_key(|(id, _)| *id);
    devices
}

pub(super) fn read_device_topology() -> Result<Vec<(u32, Option<String>)>> {
    Ok(device_topology(
        ca::device_ids()?
            .into_iter()
            .map(|id| (id, ca::device_uid(id).ok())),
    ))
}

pub(super) fn tap_mode(scoped_output: bool) -> &'static str {
    if scoped_output {
        "device_stream"
    } else {
        "global_mixdown"
    }
}
pub(super) fn scoped_initializer_supported(class: &AnyClass, selected: bool) -> bool {
    if selected {
        class.responds_to(sel!(initWithProcesses:andDeviceUID:withStream:))
    } else {
        class.responds_to(sel!(initExcludingProcesses:andDeviceUID:withStream:))
    }
}

pub(super) fn capture_progress_timeout(
    buffer_frames: Option<u32>,
    sample_rate: u32,
) -> std::time::Duration {
    let frames = buffer_frames.unwrap_or(512).max(1);
    let period = std::time::Duration::from_secs_f64(frames as f64 / sample_rate.max(1) as f64);
    (period * 3 + std::time::Duration::from_millis(100))
        .max(std::time::Duration::from_secs(2))
        .min(std::time::Duration::from_secs(5))
}

pub(super) fn wait_capture_ready(
    metrics: &Metrics,
    before: u64,
    timeout: std::time::Duration,
    mut check_configuration: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ensure!(
            metrics.errors.load(Ordering::Acquire) == 0,
            "System-audio capture callback failed"
        );
        ensure!(
            !check_configuration()?,
            "Audio configuration changed during capture startup"
        );
        // Native configuration queries can overlap another callback or listener.
        // New PCM must not hide an error or a notification that arrived while checking.
        ensure!(
            metrics.errors.load(Ordering::Acquire) == 0,
            "System-audio capture callback failed during startup validation"
        );
        if metrics.captured_frames.load(Ordering::Acquire) > before
            && metrics.configuration_events.load(Ordering::Acquire)
                == metrics.checked_configuration_events.load(Ordering::Acquire)
        {
            return Ok(());
        }
        ensure!(
            std::time::Instant::now() < deadline,
            "System-audio tap started but delivered no PCM; stopping capture to restore original playback (captured_frames={}, callback_errors={}, configuration_events={}, checked_configuration_events={})",
            metrics.captured_frames.load(Ordering::Acquire),
            metrics.errors.load(Ordering::Acquire),
            metrics.configuration_events.load(Ordering::Acquire),
            metrics.checked_configuration_events.load(Ordering::Acquire),
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
