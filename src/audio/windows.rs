//! Windows WASAPI loopback capture feeding the existing Maris renderer.
//!
//! Capture is normalized by the Windows-only flexaudio backend to 48 kHz stereo.
//! The Maris output callback still owns DSP, fades, device correction and telemetry.
use super::{bridge, devices, output_binding, Metrics, OutputBinding, Session, Update};
use crate::{analysis::AnalysisWorker, control::store::Store, dsp::Settings};
use anyhow::{ensure, Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_queue::ArrayQueue;
use flexaudio::{OutputFormat, ProcessMode, SourceKind, StreamConfig};
use std::{
    sync::{atomic::Ordering, Arc},
    thread,
    time::{Duration, Instant},
};

const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

pub(super) struct CaptureWorker {
    worker: super::worker::Worker,
    pids: Vec<i32>,
}
impl CaptureWorker {
    fn config(pid: Option<i32>) -> Result<StreamConfig> {
        let mut config = StreamConfig::default();
        config.output = OutputFormat {
            sample_rate: RATE,
            channels: CHANNELS,
        };
        config.ring_capacity_chunks = 50;
        match pid {
            Some(pid) => {
                ensure!(pid > 0, "Invalid Windows application PID");
                config.kind = SourceKind::ProcessLoopback;
                config.target_pid = Some(pid as u32);
                config.mode = ProcessMode::Include;
                config.exclude_self = false;
            }
            None => {
                config.kind = SourceKind::SystemLoopback;
                // Use the process-loopback exclusion primitive for the normal system path so
                // Maris never captures the audio it is rendering back to the selected endpoint.
                config.exclude_self = true;
                config.target_pid = None;
            }
        }
        Ok(config)
    }

    pub fn start(
        pids: &[i32],
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        // Process loopback includes descendants. Reject overlapping scopes and
        // any branch containing Maris before opening a capture stream.
        super::windows_route::validate_capture_pids(pids)?;
        let configs: Vec<_> = if pids.is_empty() {
            vec![Self::config(None)?]
        } else {
            pids.iter()
                .copied()
                .map(Some)
                .map(Self::config)
                .collect::<Result<_>>()?
        };
        let counters = metrics.clone();
        let worker = super::worker::Worker::start(
            "maris-wasapi-capture",
            Duration::from_secs(8),
            move |stop| {
                let mut streams = Vec::with_capacity(configs.len());
                for config in configs {
                    ensure!(
                        !stop.load(Ordering::Acquire),
                        "Windows capture startup cancelled"
                    );
                    let mut stream =
                        flexaudio::open(config).context("Open Windows WASAPI loopback")?;
                    ensure!(
                        !stop.load(Ordering::Acquire),
                        "Windows capture startup cancelled"
                    );
                    stream.start().context("Start Windows WASAPI loopback")?;
                    streams.push(stream);
                }
                Ok(streams)
            },
            move |mut streams, thread_stop| {
                let mut pending: Vec<Option<super::windows_mix::Block>> =
                    (0..streams.len()).map(|_| None).collect();
                let mut last_progress = Instant::now();
                while !thread_stop.load(Ordering::Acquire) {
                    let started_at = Instant::now();
                    for (index, stream) in streams.iter_mut().enumerate() {
                        if pending[index].is_none() {
                            if let Some(chunk) = stream.poll_chunk() {
                                if chunk.dropped_before > 0
                                    || chunk.flags.intersects(
                                        flexaudio::ChunkFlags::DISCONTINUITY
                                            | flexaudio::ChunkFlags::RECOVERED,
                                    )
                                {
                                    // Never let the downstream sinc history span a reported
                                    // native gap merely because the block length is valid.
                                    counters
                                        .capture_discontinuities
                                        .fetch_add(1, Ordering::Release);
                                }
                                match super::windows_mix::Block::new(chunk.data, chunk.frames) {
                                    Ok(block) => pending[index] = Some(block),
                                    Err(_) => {
                                        counters.errors.fetch_add(1, Ordering::Relaxed);
                                        thread_stop.store(true, Ordering::Release);
                                    }
                                }
                            }
                        }
                        for _ in 0..128 {
                            let Some(event) = stream.poll_event() else {
                                break;
                            };
                            match event {
                                flexaudio::Event::Error(_)
                                | flexaudio::Event::PermissionDenied
                                | flexaudio::Event::DeviceLost => {
                                    counters.errors.fetch_add(1, Ordering::Relaxed);
                                    thread_stop.store(true, Ordering::Release);
                                }
                                flexaudio::Event::ChunkDropped { count } => {
                                    counters.overruns.fetch_add(count, Ordering::Relaxed);
                                }
                                _ => {}
                            }
                        }
                    }
                    if thread_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let progress_frames = match super::windows_mix::mix(
                        &mut pending,
                        &mut last_progress,
                        Instant::now(),
                        |mixed| {
                            if queue.push(mixed).is_err() {
                                counters.overruns.fetch_add(1, Ordering::Relaxed);
                            }
                        },
                    ) {
                        Ok(frames) => frames,
                        Err(_) => {
                            counters.errors.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    };
                    counters
                        .captured_frames
                        .fetch_add(progress_frames as u64, Ordering::Relaxed);
                    let elapsed = started_at.elapsed().as_nanos() as u64;
                    counters
                        .worker_processing_nanos
                        .fetch_add(elapsed, Ordering::Relaxed);
                    if progress_frames > 0 {
                        counters
                            .worker_frames
                            .fetch_add(progress_frames as u64, Ordering::Relaxed);
                    } else {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                for stream in &mut streams {
                    stream.stop();
                }
            },
        )?;
        Ok(Self {
            worker,
            pids: pids.to_vec(),
        })
    }

    pub fn healthy(&self) -> bool {
        // Historical diagnostic counters must not poison a successfully restored worker.
        self.worker.healthy()
    }

    pub fn pids(&self) -> &[i32] {
        &self.pids
    }
}
pub(super) struct Native {
    capture: Option<CaptureWorker>,
    route: Option<super::windows_route::Guard>,
    queue: Arc<ArrayQueue<[f32; 2]>>,
    pub selected: Option<String>,
    capture_metrics: Arc<Metrics>,
    device_format: (u32, u16, cpal::SampleFormat),
    checked: Instant,
}
impl Native {
    pub(super) fn capture_metrics(&self) -> &Metrics {
        &self.capture_metrics
    }
    pub fn healthy(&self) -> bool {
        self.capture.as_ref().is_some_and(CaptureWorker::healthy)
            && self
                .route
                .as_ref()
                .is_some_and(super::windows_route::Guard::healthy)
    }
    pub fn pids(&self) -> &[i32] {
        self.capture.as_ref().map_or(&[], CaptureWorker::pids)
    }
}

struct OutputPipeline {
    stream: cpal::Stream,
    metrics: Arc<Metrics>,
    updates: Arc<ArrayQueue<Update>>,
    analysis: AnalysisWorker,
    binding: OutputBinding,
    name: String,
    rate: u32,
    device_format: (u32, u16, cpal::SampleFormat),
    revision: u64,
    music_revision: u64,
    capability: crate::devices::capability::Capability,
}
fn device_format(device: &cpal::Device) -> Result<(u32, u16, cpal::SampleFormat)> {
    let config = device.default_output_config()?;
    Ok((
        config.sample_rate().0,
        config.channels(),
        config.sample_format(),
    ))
}
fn output_pipeline(
    store: &Store,
    output: Option<&str>,
    queue: Arc<ArrayQueue<[f32; 2]>>,
    capture_metrics: Arc<Metrics>,
) -> Result<OutputPipeline> {
    let device = devices::select(output, false)?;
    let name = device.name()?;
    ensure!(
        !devices::is_virtual(&name),
        "Choose physical headphones or speakers as Maris output"
    );
    let device_format = device_format(&device)?;
    let format = devices::config_near(&device, false, RATE)?;
    let rate = format.sample_rate().0;
    let binding = output_binding(store, output, &name, &device)?;
    let state = store.load()?;
    let library = crate::tuning::preferences::load(store)?;
    let capability = crate::devices::capability::effective(store, &binding.resolution.profile_key)?;
    let music = crate::devices::capability::apply_constraints(
        library.effective(&binding.resolution.profile_key),
        &capability,
    );
    let baseline = Settings::compile(&state.profile, rate)?
        .with_music(&music.output_switch_baseline(), rate)?;
    let target = Settings::compile(&state.profile, rate)?
        .with_music(&music, rate)?
        .with_transition_ms(rate, 120);
    let (baseline, target) = baseline.share_transition_headroom(target);
    let analysis = AnalysisWorker::start(rate)?;
    let metrics = Arc::new(Metrics {
        analysis_dropped: analysis.dropped.clone(),
        ..Metrics::default()
    });
    let _ = metrics.analysis_queue.set(analysis.queue.clone());
    metrics.output_quarantined.store(true, Ordering::Release);
    let updates = Arc::new(ArrayQueue::new(8));
    ensure!(
        updates
            .push(Update {
                settings: target,
                revision: state.revision,
                music_revision: library.revision
            })
            .is_ok(),
        "Output transition queue unavailable"
    );
    let source =
        bridge::LiveSource::with_rates(queue, RATE, rate)?.with_capture_metrics(capture_metrics);
    let stream = bridge::output(
        &device,
        &format,
        bridge::Source::Live(source),
        baseline,
        updates.clone(),
        metrics.clone(),
    )?;
    Ok(OutputPipeline {
        stream,
        metrics,
        updates,
        analysis,
        binding,
        name,
        rate,
        device_format,
        revision: state.revision,
        music_revision: library.revision,
        capability,
    })
}

impl Session {
    pub(in crate::audio) fn native_windows(
        store: Store,
        pids: &[i32],
        output: Option<&str>,
    ) -> Result<Self> {
        let lease = store.session_lock()?;
        let capture_metrics = Arc::new(Metrics::default());
        let queue = Arc::new(ArrayQueue::new((RATE / 4) as usize));
        let p = output_pipeline(&store, output, queue.clone(), capture_metrics.clone())?;
        let context = crate::analysis::context::Worker::start()?;
        p.stream.play()?;
        bridge::wait_output_ready(&p.metrics, Duration::from_secs(2))?;
        let capture = CaptureWorker::start(pids, queue.clone(), capture_metrics.clone())?;
        let route = super::windows_route::Guard::start(pids)?;
        let selected = output.and_then(|_| {
            p.binding
                .resolution
                .identity
                .stable_id
                .as_ref()
                .map(|id| format!("wasapi:{id}"))
        });
        p.metrics.output_quarantined.store(false, Ordering::Release);
        Ok(Self {
            streams: vec![p.stream],
            route: None,
            _lease: lease,
            updates: p.updates,
            metrics: p.metrics,
            store,
            input_name: if pids.is_empty() {
                "Windows system playback (WASAPI loopback)".into()
            } else {
                "Selected Windows applications (WASAPI process loopback)".into()
            },
            output_name: p.name,
            output_profile_key: p.binding.resolution.profile_key,
            output_identity: p.binding.resolution.identity,
            output_binding_source: p.binding.resolution.binding_source,
            output_binding_revision: p.binding.resolution.binding_revision,
            output_buffer_frames: p.binding.buffer_frames,
            output_latency_frames: p.binding.latency_frames,
            output_safety_offset_frames: p.binding.safety_offset_frames,
            rebind_count: 0,
            last_rebind_ms: None,
            sample_rate: p.rate,
            sent_revision: p.revision,
            sent_music_revision: Some(p.music_revision),
            sent_capability: p.capability,
            last_status: Instant::now() - Duration::from_secs(1),
            completion: None,
            analysis: p.analysis,
            context,
            voice: None,
            #[cfg(unix)]
            pulse: None,
            windows: Some(Native {
                capture: Some(capture),
                route: Some(route),
                queue,
                selected,
                capture_metrics,
                device_format: p.device_format,
                checked: Instant::now(),
            }),
            session_id: super::new_session_id(),
            stop_requested: false,
            device_match_started: false,
            last_control_result: None,
            #[cfg(target_os = "macos")]
            tap: None,
            #[cfg(target_os = "macos")]
            system_output: None,
            #[cfg(target_os = "macos")]
            system_processes: Vec::new(),
            #[cfg(target_os = "macos")]
            system_process_pids: Vec::new(),
            #[cfg(target_os = "macos")]
            system_device_id: 0,
            #[cfg(target_os = "macos")]
            last_device_check: Instant::now(),
        })
    }

    pub(in crate::audio) fn rebind_windows_processes(&mut self, pids: &[i32]) -> Result<()> {
        // Validate the complete process tree before fading or releasing a healthy
        // route. The capture worker rechecks after teardown because apps can exit.
        super::windows_route::validate_capture_pids(pids)?;
        let (queue, previous, capture_metrics) = {
            let native = self
                .windows
                .as_mut()
                .context("No active Windows loopback session")?;
            let previous = native.pids().to_vec();
            self.metrics.stopping.store(true, Ordering::Release);
            let until = Instant::now() + Duration::from_millis(120);
            while !self.metrics.faded_out.load(Ordering::Acquire) && Instant::now() < until {
                thread::sleep(Duration::from_millis(2));
            }
            self.metrics
                .output_quarantined
                .store(true, Ordering::Release);
            native.capture.take();
            native.route.take();
            for _ in 0..native.queue.capacity() {
                if native.queue.pop().is_none() {
                    break;
                }
            }
            native
                .capture_metrics
                .capture_discontinuities
                .fetch_add(1, Ordering::Release);
            (
                native.queue.clone(),
                previous,
                native.capture_metrics.clone(),
            )
        };
        let replacement = CaptureWorker::start(pids, queue.clone(), capture_metrics.clone())
            .and_then(|capture| {
                let route = super::windows_route::Guard::start(pids)?;
                Ok((capture, route))
            });
        match replacement {
            Ok((worker, route)) => {
                let native = self.windows.as_mut().expect("active Windows session");
                native.capture = Some(worker);
                native.route = Some(route);
                self.metrics.stopping.store(false, Ordering::Release);
                self.metrics
                    .output_quarantined
                    .store(false, Ordering::Release);
                self.rebind_count = self.rebind_count.saturating_add(1);
                self.last_rebind_ms = Some(crate::analysis::now_ms());
                self.context.reset();
                self.input_name = if pids.is_empty() {
                    "Windows system playback (WASAPI loopback)".into()
                } else {
                    "Selected Windows applications (WASAPI process loopback)".into()
                };
                Ok(())
            }
            Err(error) => {
                for _ in 0..queue.capacity() {
                    if queue.pop().is_none() {
                        break;
                    }
                }
                capture_metrics
                    .capture_discontinuities
                    .fetch_add(1, Ordering::Release);
                let restored =
                    CaptureWorker::start(&previous, queue, capture_metrics).and_then(|capture| {
                        let route = super::windows_route::Guard::start(&previous)?;
                        Ok((capture, route))
                    });
                match restored {
                    Ok((capture, route)) => {
                        let native = self.windows.as_mut().expect("active Windows session");
                        native.capture = Some(capture);
                        native.route = Some(route);
                        self.metrics.stopping.store(false, Ordering::Release);
                        self.metrics.output_quarantined.store(false, Ordering::Release);
                        Err(error)
                    }
                    Err(restore) => Err(anyhow::anyhow!("Application switch failed: {error:#}; previous scope restoration failed: {restore:#}")),
                }
            }
        }
    }

    pub(in crate::audio) fn rebind_windows_output(&mut self, output: Option<&str>) -> Result<()> {
        let native = self
            .windows
            .as_ref()
            .context("No active Windows loopback session")?;
        let p = output_pipeline(
            &self.store,
            output,
            native.queue.clone(),
            native.capture_metrics.clone(),
        )?;
        let selected = output.and_then(|_| {
            p.binding
                .resolution
                .identity
                .stable_id
                .as_ref()
                .map(|id| format!("wasapi:{id}"))
        });
        bridge::handoff_output(
            &mut self.streams,
            p.stream,
            &self.metrics,
            &p.metrics,
            |stream| stream.play().map_err(Into::into),
        )?;
        // Publish only the successfully handed-off output; failed candidates never
        // overwrite active revisions, telemetry, sample rate or analysis state.
        self.metrics = p.metrics;
        self.analysis = p.analysis;
        self.updates = p.updates;
        self.output_name = p.name;
        self.output_profile_key = p.binding.resolution.profile_key;
        self.output_identity = p.binding.resolution.identity;
        self.output_binding_source = p.binding.resolution.binding_source;
        self.output_binding_revision = p.binding.resolution.binding_revision;
        self.output_buffer_frames = p.binding.buffer_frames;
        self.output_latency_frames = p.binding.latency_frames;
        self.output_safety_offset_frames = p.binding.safety_offset_frames;
        self.sample_rate = p.rate;
        self.sent_revision = p.revision;
        self.sent_music_revision = Some(p.music_revision);
        self.sent_capability = p.capability;
        self.rebind_count = self.rebind_count.saturating_add(1);
        self.last_rebind_ms = Some(crate::analysis::now_ms());
        self.device_match_started = false;
        self.context.reset();
        let native = self.windows.as_mut().expect("active Windows session");
        native.selected = selected;
        native.device_format = p.device_format;
        Ok(())
    }

    pub(super) fn refresh_windows_output(&mut self) -> Result<()> {
        let Some(native) = self.windows.as_mut() else {
            return Ok(());
        };
        if native.checked.elapsed() < Duration::from_millis(300)
            && self.metrics.errors.load(Ordering::Acquire) == 0
        {
            return Ok(());
        }
        native.checked = Instant::now();
        let inventory = super::windows_devices::devices()?;
        let target = devices::follow_stable_output(&inventory, native.selected.as_deref())?;
        let target_id = target.id.clone();
        let device = super::windows_devices::select(&target_id, false)?;
        let format = device_format(&device)?;
        let disappeared = native.selected.as_ref().is_some_and(|id| id != &target_id);
        let unchanged = self.output_identity.stable_id.as_deref()
            == target_id.strip_prefix("wasapi:")
            && native.device_format == format;
        if unchanged && self.metrics.errors.load(Ordering::Acquire) == 0 {
            return Ok(());
        }
        let pin = native.selected.is_some() && !disappeared;
        // Resolve the default to the observed stable ID for construction. Only a
        // successful handoff changes follow/pin state; failures keep the prior output.
        match self.rebind_windows_output(Some(&target_id)) {
            Ok(()) => {
                if !pin {
                    self.windows
                        .as_mut()
                        .expect("active Windows session")
                        .selected = None;
                }
                Ok(())
            }
            Err(error)
                if self.metrics.errors.load(Ordering::Acquire) == 0 && !self.streams.is_empty() =>
            {
                self.record_control_result(
                    "select_output",
                    Some(format!("Automatic output change failed: {error:#}")),
                );
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

pub(super) fn applications() -> Result<serde_json::Value> {
    let current = std::process::id();
    let applications = super::windows_route::processes()?
        .into_iter()
        .filter(|process| process.pid != current)
        .map(|process| {
            serde_json::json!({
                "object_id":0,
                "pid":process.pid,
                "bundle_id":serde_json::Value::Null,
                "name":process.name,
                "executable":process.executable,
                "running_output":process.is_output_active,
                "devices":[],
                "is_maris":false
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "available":true,
        "backend":"wasapi_audio_sessions",
        "applications":applications,
        "per_application_capture_backend":"wasapi_process_loopback",
        "per_application_capture":true,
        "per_application_routing":true,
        "per_application_volume":true,
        "changes_application_session_mute":true,
        "note":"Each selected PID can feed an independent Maris mixer strip. While Maris replaces an application route, its original Windows render session is temporarily muted and its prior mute state is restored on stop; volume scalars and the default output are not changed."
    }))
}

pub(super) fn doctor() -> serde_json::Value {
    match super::windows_route::processes() {
        Ok(processes) => serde_json::json!({
            "available":true,
            "backend":"wasapi_process_loopback",
            "driver_required":false,
            "process_enumeration":true,
            "processes":processes.len(),
            "capture_started":false,
            "changes_default_output":false,
            "changes_volume_scalar":false,
            "changes_application_session_mute_when_active":true
        }),
        Err(error) => serde_json::json!({
            "available":false,
            "backend":"wasapi_process_loopback",
            "driver_required":false,
            "process_enumeration":false,
            "capture_started":false,
            "reason":format!("{error:#}")
        }),
    }
}

pub(super) struct ApplicationCapture {
    worker: Option<CaptureWorker>,
    route: Option<super::windows_route::Guard>,
    pid: i32,
    queue: Arc<ArrayQueue<[f32; 2]>>,
    metrics: Arc<Metrics>,
}
impl ApplicationCapture {
    pub fn prepare(
        pid: i32,
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        ensure!(pid > 0, "Invalid Windows application PID");
        Ok(Self {
            worker: None,
            route: None,
            pid,
            queue,
            metrics,
        })
    }
    pub fn start(&mut self) -> Result<()> {
        ensure!(
            self.worker.is_none() && self.route.is_none(),
            "Application capture already started"
        );
        // Capture is armed first; only after it succeeds do we mute the original render session.
        // If route activation fails, dropping the temporary worker leaves the application audible.
        let worker = CaptureWorker::start(&[self.pid], self.queue.clone(), self.metrics.clone())?;
        let route = match super::windows_route::Guard::start(&[self.pid]) {
            Ok(route) => route,
            Err(error) => {
                drop(worker);
                return Err(error);
            }
        };
        self.worker = Some(worker);
        self.route = Some(route);
        Ok(())
    }
    pub fn check(&self) -> Result<()> {
        ensure!(
            self.worker.as_ref().is_some_and(CaptureWorker::healthy),
            "Windows application capture stopped"
        );
        ensure!(
            self.route
                .as_ref()
                .is_some_and(super::windows_route::Guard::healthy),
            "Windows application route guard stopped"
        );
        Ok(())
    }
}
