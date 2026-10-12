//! Native system route assembly and automatic default-output rebinding.
use super::{
    bridge, devices, output_binding, tap::TapCapture, tap_ffi, AnalysisWorker, Metrics,
    OutputBinding, Session, Settings, Update,
};
use crate::control::store::Store;
use anyhow::{ensure, Context, Result};
use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Stream,
};
use crossbeam_queue::ArrayQueue;
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

struct Pipeline {
    stream: Stream,
    tap: TapCapture,
    metrics: Arc<Metrics>,
    updates: Arc<ArrayQueue<Update>>,
    analysis: AnalysisWorker,
    output: String,
    binding: OutputBinding,
    revision: u64,
    music_revision: u64,
    capability: crate::devices::capability::Capability,
}
fn pipeline(store: &Store, output: Option<&str>, processes: &[u32]) -> Result<Pipeline> {
    let device = devices::select(output, false)?;
    let name = device.description()?.name().to_owned();
    let mut binding = output_binding(store, output, &name, &device)?;
    ensure!(!devices::is_virtual(&name), "The active system output is a loopback device, not physical playback; original audio was not changed");
    let rate = device.default_output_config()?.sample_rate();
    let initial = devices::config(&device, false, rate)?;
    let state = store.load()?;
    let queue = Arc::new(ArrayQueue::new(192000 / 4));
    let metrics = Arc::new(Metrics::default());
    let updates = Arc::new(ArrayQueue::<Update>::new(8));
    let initial_buffer_frames = binding
        .coreaudio_id
        .and_then(|device| tap_ffi::output_buffer_frames(device).ok());
    let initial_progress_timeout =
        output_progress_timeout(initial_buffer_frames, rate, Duration::from_secs(1));
    // Register Maris as an audio process before resolving the exclusion PID. This stream emits only silence.
    let prime_metrics = Arc::new(Metrics::default());
    let prime = bridge::output(
        &device,
        &initial,
        bridge::Source::Live(bridge::LiveSource::new(queue.clone(), rate)),
        Settings::compile(&state.profile, rate)?,
        Arc::new(ArrayQueue::new(8)),
        prime_metrics.clone(),
    )?;
    prime.play()?;
    bridge::wait_output_ready(&prime_metrics, initial_progress_timeout)?;
    // System-wide capture must stay on the established global tap. Some CoreAudio
    // endpoints accept the device-scoped CATapDescription but then mute that physical
    // stream when CATapMutedWhenTapped is engaged, leaving Maris with no usable PCM while
    // the original application audio is suppressed. Device-specific silence reports
    // do not establish that every endpoint has this failure.
    //
    // Keep device-scoped capture only for explicit per-application capture. System mode
    // uses a global self-excluding tap and still renders to the selected physical output.
    let output_uid = binding
        .coreaudio_id
        .and_then(|device| tap_ffi::device_uid(device).ok())
        .context("Selected macOS output has no stable CoreAudio UID")?;
    let mut tap = if processes.is_empty() {
        TapCapture::prepare_for_output_clock(queue.clone(), metrics.clone(), &output_uid)?
    } else if let Some(uid) = binding.coreaudio_id.and_then(|device| {
        tap_ffi::single_stereo_output_device_uid(device)
            .ok()
            .flatten()
    }) {
        TapCapture::prepare_processes_for_output(queue.clone(), metrics.clone(), processes, &uid)?
    } else {
        TapCapture::prepare_processes(queue.clone(), metrics.clone(), processes)?
    };
    // Keep the silent registration stream alive until the replacement has
    // actually delivered its own callback. Stopping the physical clock here
    // forces devices to cold-start again while their aggregate is being built.
    // Separate metrics and queues prevent priming callbacks from acknowledging
    // replacement readiness or consuming its pending settings.
    let config = devices::config(&device, false, tap.rate)?;
    metrics.frames.store(0, Ordering::Relaxed);
    metrics.underruns.store(0, Ordering::Relaxed);
    metrics
        .callback_processing_nanos
        .store(0, Ordering::Relaxed);
    metrics.callback_max_nanos.store(0, Ordering::Relaxed);
    metrics.callback_frames.store(0, Ordering::Relaxed);
    metrics.callback_calls.store(0, Ordering::Relaxed);
    let analysis = AnalysisWorker::start_with_dropped(tap.rate, metrics.analysis_dropped.clone())?;
    let _ = metrics.analysis_queue.set(analysis.queue.clone());
    metrics.revision.store(state.revision, Ordering::Relaxed);
    let listening = crate::tuning::preferences::load(store)?;
    let profile_key = &binding.resolution.profile_key;
    let capability = crate::devices::capability::effective(store, profile_key)?;
    let effective_music = crate::devices::capability::apply_constraints(
        listening.effective(profile_key),
        &capability,
    );
    // A new physical endpoint should never appear with the full subjective target instantly.
    // Start with the same device correction but neutral preference controls, then crossfade into
    // the saved target over 120 ms. This keeps correction evidence intact while avoiding a sharp
    // tonal jump when switching outputs or recovering from hotplug.
    let switch_baseline = effective_music.output_switch_baseline();
    let startup_settings =
        Settings::compile(&state.profile, tap.rate)?.with_music(&switch_baseline, tap.rate)?;
    let target_settings = Settings::compile(&state.profile, tap.rate)?
        .with_music(&effective_music, tap.rate)?
        .with_transition_ms(tap.rate, 120);
    let (startup_settings, target_settings) =
        startup_settings.share_transition_headroom(target_settings);
    metrics
        .music_revision
        .store(listening.revision, Ordering::Relaxed);
    let stream = bridge::output(
        &device,
        &config,
        bridge::Source::Live(bridge::LiveSource::system(queue, tap.rate)),
        startup_settings,
        updates.clone(),
        metrics.clone(),
    )?;
    ensure!(
        updates
            .push(Update {
                settings: target_settings,
                revision: state.revision,
                music_revision: listening.revision,
            })
            .is_ok(),
        "Internal output-switch transition queue is unavailable"
    );
    if let Some(device_id) = binding.coreaudio_id {
        tap.watch_output(device_id, output.is_none())?;
        // Capture the negotiated output, not the pre-construction buffer snapshot.
        binding.buffer_frames = tap_ffi::output_buffer_frames(device_id).ok();
        binding.latency_frames = tap_ffi::output_latency_frames(device_id).ok();
        binding.safety_offset_frames = tap_ffi::output_safety_offset_frames(device_id).ok();
    }
    // Prove the replacement callback while forcing it to silence. The original
    // application path stays audible during this entire preflight.
    metrics.output_quarantined.store(true, Ordering::Release);
    stream.play()?;
    bridge::wait_output_ready(&metrics, Duration::from_secs(1))?;
    drop(prime);

    // Start the tap unmuted and require real PCM before changing original playback.
    tap.start()?;
    tap.arm_replacement()?;
    metrics.output_quarantined.store(false, Ordering::Release);
    // A live tap plus a live output callback still is not enough: require the clock
    // bridge and startup envelope to observe post-handoff source frames.
    let handoff_timeout =
        output_progress_timeout(binding.buffer_frames, tap.rate, Duration::from_secs(1));
    if let Err(start_error) = bridge::wait_source_started(&metrics, handoff_timeout) {
        // Fail open in a deterministic order. tap was constructed before stream, so
        // ordinary stack unwinding would drop the replacement output first and only then
        // restore the original application path from TapCapture::drop. Re-quarantine the
        // replacement and explicitly restore original playback while that stream is still
        // alive. If the live property update itself fails, destroy the tap now (also while
        // the replacement stream is alive) so CoreAudio gets a second restoration path
        // before the output disappears.
        metrics.output_quarantined.store(true, Ordering::Release);
        if let Err(restore_error) = tap.disarm_replacement() {
            drop(tap);
            return Err(start_error.context(format!(
                "Replacement output never received post-handoff audio; restoring the original CoreAudio tap also failed: {restore_error:#}"
            )));
        }
        return Err(start_error);
    }
    Ok(Pipeline {
        stream,
        tap,
        metrics,
        updates,
        analysis,
        output: name,
        binding,
        revision: state.revision,
        music_revision: listening.revision,
        capability,
    })
}
#[derive(Clone, Copy)]
pub(super) struct ProgressGuard {
    pub(super) output_callbacks: u64,
    pub(super) capture_callbacks: u64,
    output_advanced_at: Instant,
    capture_advanced_at: Instant,
}

fn output_progress_timeout(
    buffer_frames: Option<u32>,
    sample_rate: u32,
    minimum: Duration,
) -> Duration {
    let frames = buffer_frames.unwrap_or(512).max(1);
    let period = Duration::from_secs_f64(frames as f64 / sample_rate.max(1) as f64);
    (period * 3 + Duration::from_millis(100))
        .max(minimum)
        .min(Duration::from_secs(5))
}

fn output_stall_timeout(buffer_frames: Option<u32>, sample_rate: u32) -> Duration {
    output_progress_timeout(buffer_frames, sample_rate, Duration::from_millis(750))
}

fn observe_progress(
    previous: Option<ProgressGuard>,
    current: (u64, u64),
    now: Instant,
    timeout: Duration,
) -> (ProgressGuard, bool) {
    let Some(previous) = previous else {
        return (
            ProgressGuard {
                output_callbacks: current.0,
                capture_callbacks: current.1,
                output_advanced_at: now,
                capture_advanced_at: now,
            },
            false,
        );
    };
    let output_advanced = current.0 != previous.output_callbacks;
    let capture_advanced = current.1 != previous.capture_callbacks;
    let output_advanced_at = if output_advanced {
        now
    } else {
        previous.output_advanced_at
    };
    let capture_advanced_at = if capture_advanced {
        now
    } else {
        previous.capture_advanced_at
    };
    // The physical replacement output must keep producing callbacks even for digital
    // silence. If it stops, fail open regardless of whether capture also stopped.
    let output_stalled =
        !output_advanced && now.saturating_duration_since(previous.output_advanced_at) >= timeout;
    // A running output with a dead tap IOProc would render starvation/silence while
    // original playback remains muted. Tap callback liveness is content-independent, so
    // a silent or paused selected application does not look like a capture failure.
    let capture_stalled = output_advanced
        && !capture_advanced
        && now.saturating_duration_since(previous.capture_advanced_at) >= timeout;
    (
        ProgressGuard {
            output_callbacks: current.0,
            capture_callbacks: current.1,
            output_advanced_at,
            capture_advanced_at,
        },
        output_stalled || capture_stalled,
    )
}

pub(super) fn handoff_status(session: &Session) -> serde_json::Value {
    let last_progress = session.last_system_progress.map_or(
        serde_json::Value::Null,
        |progress| {
            serde_json::json!({
                "output_callbacks":progress.output_callbacks,
                "capture_callbacks":progress.capture_callbacks
            })
        },
    );
    serde_json::json!({
        "original_playback_muted":session.tap.as_ref().is_some_and(TapCapture::replacement_armed),
        "output_quarantined":session.metrics.output_quarantined.load(Ordering::Acquire),
        "output_callback_ready":session.metrics.callback_ready.load(Ordering::Acquire),
        "output_callback_calls":session.metrics.callback_calls.load(Ordering::Acquire),
        "capture_callback_calls":session.metrics.capture_callback_calls.load(Ordering::Acquire),
        "captured_frames":session.metrics.captured_frames.load(Ordering::Acquire),
        "source_started":session.metrics.source_started.load(Ordering::Acquire),
        "last_progress":last_progress
    })
}


impl Session {
    pub(super) fn native_system(store: Store, output: Option<&str>) -> Result<Self> {
        Self::native_tap(
            store,
            output,
            Vec::new(),
            Vec::new(),
            "System audio (native tap)".into(),
        )
    }

    pub(super) fn native_applications(
        store: Store,
        pids: &[i32],
        output: Option<&str>,
    ) -> Result<Self> {
        ensure!(
            !pids.is_empty() && pids.len() <= 64,
            "Select between 1 and 64 application PIDs"
        );
        let own_pid = std::process::id() as i32;
        let mut processes = Vec::with_capacity(pids.len());
        for pid in pids {
            ensure!(*pid > 0 && *pid != own_pid, "Invalid application PID {pid}");
            let object = tap_ffi::process_object_for_pid(*pid)?;
            ensure!(
                !processes.contains(&object),
                "Duplicate application audio process {pid}"
            );
            processes.push(object);
        }
        Self::native_tap(
            store,
            output,
            processes,
            pids.to_vec(),
            format!(
                "Applications (native tap): {}",
                pids.iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        )
    }

    fn native_tap(
        store: Store,
        output: Option<&str>,
        system_processes: Vec<u32>,
        system_process_pids: Vec<i32>,
        input_name: String,
    ) -> Result<Self> {
        let lease = store.session_lock()?;
        let p = pipeline(&store, output, &system_processes)?;
        let sample_rate = p.tap.rate;
        let system_device_id = p.binding.coreaudio_id.unwrap_or(tap_ffi::default_output()?);
        Ok(Self {
            streams: vec![p.stream],
            route: None,
            tap: Some(p.tap),
            _lease: lease,
            updates: p.updates,
            metrics: p.metrics,
            store,
            input_name,
            output_name: p.output,
            output_profile_key: p.binding.resolution.profile_key,
            output_identity: p.binding.resolution.identity,
            output_binding_source: p.binding.resolution.binding_source,
            output_binding_revision: p.binding.resolution.binding_revision,
            output_buffer_frames: p.binding.buffer_frames,
            output_latency_frames: p.binding.latency_frames,
            output_safety_offset_frames: p.binding.safety_offset_frames,
            rebind_count: 0,
            last_rebind_ms: None,
            sample_rate,
            sent_revision: p.revision,
            sent_music_revision: Some(p.music_revision),
            sent_capability: p.capability,
            last_status: Instant::now() - Duration::from_secs(1),
            completion: None,
            analysis: p.analysis,
            context: crate::analysis::context::Worker::start()?,
            voice: None,
            pulse: None,
            session_id: super::new_session_id(),
            stop_requested: false,
            device_match_started: false,
            last_control_result: None,
            system_output: output.map(str::to_owned),
            system_processes,
            system_process_pids,
            system_device_id,
            last_device_check: Instant::now(),
            last_system_progress: None,
        })
    }
    fn install_system_pipeline(&mut self, p: Pipeline) {
        self.sample_rate = p.tap.rate;
        self.tap = Some(p.tap);
        self.streams = vec![p.stream];
        self.metrics = p.metrics;
        self.updates = p.updates;
        self.analysis = p.analysis;
        self.output_name = p.output;
        self.output_profile_key = p.binding.resolution.profile_key;
        self.output_identity = p.binding.resolution.identity;
        self.output_binding_source = p.binding.resolution.binding_source;
        self.output_binding_revision = p.binding.resolution.binding_revision;
        self.output_buffer_frames = p.binding.buffer_frames;
        self.output_latency_frames = p.binding.latency_frames;
        self.output_safety_offset_frames = p.binding.safety_offset_frames;
        self.system_device_id = p.binding.coreaudio_id.unwrap_or(0);
        self.sent_revision = p.revision;
        self.sent_music_revision = Some(p.music_revision);
        self.sent_capability = p.capability;
        self.device_match_started = false;
        self.last_system_progress = None;
        self.rebind_count = self.rebind_count.saturating_add(1);
        self.last_rebind_ms = Some(crate::analysis::now_ms());
        self.context.reset();
    }

    pub(super) fn rebind_system_processes(&mut self, pids: &[i32]) -> Result<()> {
        ensure!(
            pids.len() <= 64,
            "At most 64 application PIDs can be selected"
        );
        let own_pid = std::process::id() as i32;
        let mut processes = Vec::with_capacity(pids.len());
        for (index, pid) in pids.iter().copied().enumerate() {
            ensure!(pid > 0 && pid != own_pid, "Invalid application PID {pid}");
            ensure!(
                !pids[..index].contains(&pid),
                "Duplicate application PID {pid}"
            );
            let object = tap_ffi::process_object_for_pid(pid)?;
            ensure!(
                !processes.contains(&object),
                "Duplicate application audio process {pid}"
            );
            processes.push(object);
        }

        let old_processes = self.system_processes.clone();
        let old_pids = self.system_process_pids.clone();
        let old_input = self.input_name.clone();
        let output = self.system_output.clone();
        self.system_processes = processes;
        self.system_process_pids = pids.to_vec();
        self.input_name = if pids.is_empty() {
            "System audio (native tap)".into()
        } else {
            format!(
                "Applications (native tap): {}",
                pids.iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        if let Err(error) = self.rebind_system_output(output.as_deref()) {
            self.system_processes = old_processes;
            self.system_process_pids = old_pids;
            self.input_name = old_input;
            let restore = self.rebind_system_output(output.as_deref());
            return match restore {
                Ok(()) => Err(error),
                Err(restore_error) => Err(anyhow::anyhow!(
                    "Application capture switch failed: {error:#}; previous capture scope could not be restored: {restore_error:#}"
                )),
            };
        }
        Ok(())
    }

    pub(super) fn rebind_system_output(&mut self, output: Option<&str>) -> Result<()> {
        let device = devices::select(output, false)?;
        let name = device.description()?.name().to_owned();
        ensure!(
            !devices::is_virtual(&name),
            "Choose a physical output device, not a loopback device"
        );
        // Wait only on the control thread. A vanished device may never acknowledge,
        // so teardown remains bounded. Fade the replacement first, then restore the
        // original application path while the replacement stream still exists, and
        // only then release that stream. This avoids both overlap and a silent gap.
        self.metrics.stopping.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_millis(75);
        while !self.metrics.faded_out.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.tap.take();
        self.streams.clear();
        let processes = self.system_processes.clone();
        match pipeline(&self.store, output, &processes) {
            Ok(p) => {
                self.install_system_pipeline(p);
                self.system_output = output.map(str::to_owned);
                Ok(())
            }
            Err(error) if output.is_some() => {
                // A requested/pinned device can disappear between discovery and stream creation.
                // Fall back to the current system default rather than leaving playback stranded.
                let fallback = pipeline(&self.store, None, &processes).map_err(|fallback_error| {
                    anyhow::anyhow!("Output switch failed: {error:#}; default fallback failed: {fallback_error:#}")
                })?;
                self.install_system_pipeline(fallback);
                self.system_output = None;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn refresh_system_output(&mut self) -> Result<()> {
        let pending_configuration = self
            .tap
            .as_ref()
            .is_some_and(TapCapture::configuration_pending);
        let periodic_check = self.last_device_check.elapsed() >= Duration::from_millis(300);
        if self.tap.is_none() || (!pending_configuration && !periodic_check) {
            return Ok(());
        }
        if periodic_check {
            let now = Instant::now();
            self.last_device_check = now;
            let progress = (
                self.metrics.callback_calls.load(Ordering::Acquire),
                self.metrics.capture_callback_calls.load(Ordering::Acquire),
            );
            let (observed, stalled) = observe_progress(
                self.last_system_progress,
                progress,
                now,
                output_stall_timeout(self.output_buffer_frames, self.sample_rate),
            );
            self.last_system_progress = Some(observed);
            if stalled {
                // Capture is advancing while the physical replacement output has stopped.
                // Quarantine any callback that unexpectedly resumes and restore original
                // playback while the replacement stream is still alive. If the live tap
                // property update fails, destroy the tap immediately rather than leaving
                // both original and replacement paths silent until later session teardown.
                self.metrics
                    .output_quarantined
                    .store(true, Ordering::Release);
                self.stop_requested = true;
                let restore_error = self
                    .tap
                    .as_mut()
                    .and_then(|tap| tap.disarm_replacement().err());
                if let Some(restore_error) = restore_error {
                    self.tap.take();
                    return Err(restore_error.context(
                        "Replacement output stalled and the live CoreAudio tap could not restore original playback before tap destruction",
                    ));
                }
                return Ok(());
            }
        }
        if self
            .tap
            .as_ref()
            .is_some_and(|tap| tap.needs_rebuild().unwrap_or(true))
        {
            let output = if self.system_output.is_some() {
                if let Some(uid) = self.output_identity.stable_id.as_deref() {
                    tap_ffi::output_selector_for_uid(uid).ok()
                } else {
                    self.system_output
                        .clone()
                        .filter(|selector| devices::select(Some(selector), false).is_ok())
                }
            } else {
                None
            };
            // A disappeared pin still follows the existing default recovery path;
            // configuration failure must not strand playback on its old selector.
            return self.rebind_system_output(output.as_deref());
        }
        if !self.system_processes.is_empty() {
            ensure!(
                self.system_processes.len() == self.system_process_pids.len(),
                "Application process state is inconsistent"
            );
            let available = tap_ffi::process_ids()?;
            let survivors: Vec<_> = self
                .system_processes
                .iter()
                .copied()
                .zip(self.system_process_pids.iter().copied())
                .filter(|(process, pid)| {
                    available.contains(process)
                        && tap_ffi::process_pid(*process).is_ok_and(|current| current == *pid)
                })
                .collect();
            if survivors.is_empty() {
                // Destroying the session releases CATapMutedWhenTapped and restores normal app playback.
                self.stop_requested = true;
                return Ok(());
            }
            if survivors.len() != self.system_processes.len() {
                self.system_processes = survivors.iter().map(|(process, _)| *process).collect();
                self.system_process_pids = survivors.iter().map(|(_, pid)| *pid).collect();
                let output = self.system_output.clone();
                return self.rebind_system_output(output.as_deref());
            }
        }
        if let Some(pinned) = self.system_output.clone() {
            if let Some(uid) = self.output_identity.stable_id.as_deref() {
                let selector = match tap_ffi::output_selector_for_uid(uid) {
                    Ok(selector) => selector,
                    Err(_) => return self.rebind_system_output(None),
                };
                let device = devices::select(Some(&selector), false)?;
                let name = device.description()?.name().to_owned();
                let device_id = tap_ffi::output_device_id_for_selector(Some(&selector), &name)?;
                let rate = tap_ffi::sample_rate(device_id)?;
                if device_id == self.system_device_id && rate == self.sample_rate {
                    if selector != pinned {
                        self.system_output = Some(selector);
                    }
                    return Ok(());
                }
                return self.rebind_system_output(Some(&selector));
            }

            let device = match devices::select(Some(&pinned), false) {
                Ok(device) => device,
                Err(_) => return self.rebind_system_output(None),
            };
            let name = device.description()?.name().to_owned();
            if let Ok(device_id) = tap_ffi::output_device_id_for_selector(Some(&pinned), &name) {
                let rate = tap_ffi::sample_rate(device_id)?;
                if device_id == self.system_device_id && rate == self.sample_rate {
                    return Ok(());
                }
                return self.rebind_system_output(Some(&pinned));
            }
            // Without stable identity, preserve a still-visible endpoint rather than guessing that
            // another same-named device should inherit the pinned route.
            return Ok(());
        }
        let device_id = tap_ffi::default_output()?;
        let rate = tap_ffi::sample_rate(device_id)?;
        if device_id == self.system_device_id && rate == self.sample_rate {
            return Ok(());
        }
        self.rebind_system_output(None)
    }
}

#[cfg(test)]
mod progress_tests {
    use super::{observe_progress, output_progress_timeout, output_stall_timeout};
    use std::time::{Duration, Instant};

    #[test]
    fn progress_guard_fails_open_on_output_capture_or_combined_stalls() {
        let start = Instant::now();
        let timeout = Duration::from_millis(750);
        let (first, stalled) = observe_progress(None, (10, 100), start, timeout);
        assert!(!stalled);

        // No side has exceeded its window yet.
        let (early, stalled) = observe_progress(
            Some(first),
            (10, 200),
            start + Duration::from_millis(600),
            timeout,
        );
        assert!(!stalled);

        // Capture advances while the replacement output is stuck.
        let (_, stalled) = observe_progress(
            Some(early),
            (10, 300),
            start + Duration::from_millis(800),
            timeout,
        );
        assert!(stalled);

        // Output advances while the tap capture is stuck.
        let (capture_base, stalled) = observe_progress(None, (20, 400), start, timeout);
        assert!(!stalled);
        let (_, stalled) = observe_progress(
            Some(capture_base),
            (21, 400),
            start + Duration::from_millis(800),
            timeout,
        );
        assert!(stalled);

        // If both sides stop, the physical replacement callback itself is enough evidence
        // to fail open; capture progress is not required as a witness.
        let (both_base, stalled) = observe_progress(None, (30, 500), start, timeout);
        assert!(!stalled);
        let (_, stalled) = observe_progress(
            Some(both_base),
            (30, 500),
            start + Duration::from_millis(800),
            timeout,
        );
        assert!(stalled);

        // Genuine progress on both sides resets both watchdog clocks.
        let (healthy, stalled) = observe_progress(None, (40, 600), start, timeout);
        assert!(!stalled);
        let (_, stalled) = observe_progress(
            Some(healthy),
            (41, 700),
            start + Duration::from_secs(2),
            timeout,
        );
        assert!(!stalled);

        // Counter discontinuity/reset is still progress, not a stall.
        let (_, stalled) = observe_progress(
            Some(healthy),
            (39, 701),
            start + Duration::from_secs(2),
            timeout,
        );
        assert!(!stalled);
    }

    #[test]
    fn progress_windows_scale_with_the_real_output_buffer_period() {
        assert_eq!(
            output_stall_timeout(Some(512), 48_000),
            Duration::from_millis(750)
        );
        assert_eq!(
            output_progress_timeout(Some(512), 48_000, Duration::from_secs(1)),
            Duration::from_secs(1)
        );
        let large = output_stall_timeout(Some(65_536), 44_100);
        let startup = output_progress_timeout(Some(65_536), 44_100, Duration::from_secs(1));
        assert!(large > Duration::from_secs(4));
        assert!(large <= Duration::from_secs(5));
        assert_eq!(startup, large);
    }
}
