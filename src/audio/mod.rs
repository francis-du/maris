pub mod offline;
pub mod platform;

mod bridge;
mod devices;
pub mod mix;
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod pulse;
mod routing;
mod session_control;
#[cfg(target_os = "macos")]
mod system;
#[cfg(target_os = "macos")]
mod tap;
#[cfg(target_os = "macos")]
mod tap_ffi;
mod voice;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod windows_devices;
#[cfg(any(target_os = "windows", test))]
mod windows_mix;
#[cfg(target_os = "windows")]
mod windows_route;
#[cfg(any(target_os = "windows", test))]
mod windows_scope;
#[cfg(any(target_os = "windows", test))]
mod worker;

use crate::{
    analysis::context as music_context,
    analysis::AnalysisWorker,
    control::store::{read_json, Store},
    devices::identity as device_identity,
    dsp::Settings,
};
use anyhow::{bail, ensure, Context, Result};
use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Stream,
};
use crossbeam_queue::ArrayQueue;
pub use devices::{devices, DeviceInfo};

pub(crate) fn console_devices() -> anyhow::Result<Vec<DeviceInfo>> {
    #[allow(unused_mut)]
    let mut list = devices()?;
    #[cfg(target_os = "linux")]
    if list.iter().any(|d| d.id.starts_with("pulse:")) {
        list.retain(|d| d.direction != "output" || d.id.starts_with("pulse:"));
    }
    Ok(list)
}
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize)]
pub struct ApplicationInfo {
    pub object_id: u32,
    pub pid: i32,
    pub bundle_id: Option<String>,
    pub running_output: bool,
    pub devices: Vec<String>,
    pub is_maris: bool,
}

pub fn applications() -> Result<Value> {
    #[cfg(target_os = "macos")]
    {
        Ok(match tap_ffi::applications() {
            Ok(applications) => json!({
                "available":true,
                "backend":"coreaudio_process_objects",
                "applications":applications,
                "per_application_capture_backend":"coreaudio_process_tap",
                "per_application_capture":true,
                "per_application_routing":true,
                "per_application_volume":true,
                "note":"Selected application PIDs can use the shared listening path or independent mixer strips with separate gain, EQ and compression."
            }),
            Err(error) => json!({
                "available":false,
                "backend":"coreaudio_process_objects",
                "applications":[],
                "per_application_capture_backend":"coreaudio_process_tap",
                "per_application_capture":false,
                "per_application_routing":false,
                "per_application_volume":false,
                "reason":format!("{error:#}"),
                "note":"CoreAudio process metadata was unavailable; no application state is invented."
            }),
        })
    }
    #[cfg(target_os = "linux")]
    {
        Ok(pulse::applications().unwrap_or_else(|error| json!({"available":false,
            "backend":"pulse_server","applications":[],"per_application_capture":false,
            "per_application_routing":false,"per_application_volume":false,"reason":format!("{error:#}")})))
    }
    #[cfg(target_os = "windows")]
    {
        windows::applications()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Ok(json!({
            "available":false,
            "backend":Value::Null,
            "applications":[],
            "per_application_capture":false,
            "per_application_routing":false,
            "per_application_volume":false,
            "note":"This operating system is outside Maris' supported macOS, Linux and Windows targets."
        }))
    }
}

#[derive(Default)]
pub(super) struct Metrics {
    peak: AtomicU32,
    peak_left: AtomicU32,
    peak_right: AtomicU32,
    frames: AtomicU64,
    captured_frames: AtomicU64,
    callback_processing_nanos: AtomicU64,
    callback_max_nanos: AtomicU64,
    callback_frames: AtomicU64,
    callback_calls: AtomicU64,
    worker_processing_nanos: AtomicU64,
    worker_frames: AtomicU64,
    analysis_queue: OnceLock<Arc<ArrayQueue<[f32; 2]>>>,
    analysis_dropped: Arc<AtomicU64>,
    underruns: AtomicU64,
    overruns: AtomicU64,
    capture_discontinuities: AtomicU64,
    callback_discontinuities: AtomicU64,
    stream_resets: AtomicU64,
    configuration_events: AtomicU64,
    checked_configuration_events: AtomicU64,
    errors: AtomicU64,
    revision: AtomicU64,
    music_revision: AtomicU64,
    effective_gain: AtomicU32,
    adaptive_reduction: AtomicU32,
    tonal_bypass: AtomicBool,
    finished: AtomicBool,
    stopping: AtomicBool,
    faded_out: AtomicBool,
    source_started: AtomicBool,
    output_quarantined: AtomicBool,
    callback_ready: AtomicBool,
}
impl Metrics {
    fn continuity(&self) -> Value {
        json!({
            "revision":"stream-recovery-1",
            "capture_discontinuities":self.capture_discontinuities.load(Ordering::Acquire),
            "callback_discontinuities":self.callback_discontinuities.load(Ordering::Acquire),
            "stream_resets":self.stream_resets.load(Ordering::Relaxed),
            "configuration_events":self.configuration_events.load(Ordering::Acquire),
            "checked_configuration_events":self.checked_configuration_events.load(Ordering::Acquire),
            "scope":"current_pipeline",
            "power_state_measured":false
        })
    }
}
#[derive(Clone, Copy)]
pub(super) struct Update {
    settings: Settings,
    revision: u64,
    music_revision: u64,
}

pub(super) struct OutputBinding {
    pub resolution: device_identity::Resolution,
    #[cfg(target_os = "macos")]
    pub coreaudio_id: Option<u32>,
    pub buffer_frames: Option<u32>,
    pub latency_frames: Option<u32>,
    pub safety_offset_frames: Option<u32>,
}

pub(super) fn output_binding(
    store: &Store,
    selector: Option<&str>,
    name: &str,
    device: &cpal::Device,
) -> Result<OutputBinding> {
    #[cfg(not(target_os = "windows"))]
    let _ = device;
    #[cfg(target_os = "macos")]
    {
        // CPAL and CoreAudio enumerate the same device list and CPAL preserves order while
        // filtering output-capable devices. Resolve the selected endpoint back to CoreAudio
        // exactly; never silently downgrade an explicit stable selector to display-name identity.
        let device_id = tap_ffi::output_device_id_for_selector(selector, name)?;
        let stable_id = tap_ffi::device_uid(device_id).ok();
        let model_id = tap_ffi::device_model_uid(device_id).ok();
        let source = if stable_id.is_some() {
            "coreaudio_device_uid"
        } else {
            "display_name"
        };
        let resolution = device_identity::resolve_or_remember(
            store,
            device_identity::Identity {
                platform: "macos".into(),
                stable_id,
                model_id,
                display_name: name.to_owned(),
                source: source.into(),
            },
        )?;
        Ok(OutputBinding {
            resolution,
            coreaudio_id: Some(device_id),
            buffer_frames: tap_ffi::output_buffer_frames(device_id).ok(),
            latency_frames: tap_ffi::output_latency_frames(device_id).ok(),
            safety_offset_frames: tap_ffi::output_safety_offset_frames(device_id).ok(),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = selector;
        #[cfg(target_os = "windows")]
        let identity = device_identity::Identity {
            platform: "windows".into(),
            stable_id: Some(windows_devices::identity(device)?),
            model_id: None,
            display_name: name.into(),
            source: "wasapi_endpoint_id".into(),
        };
        #[cfg(not(target_os = "windows"))]
        let identity = device_identity::Identity::display_name(name);
        Ok(OutputBinding {
            resolution: device_identity::resolve_or_remember(store, identity)?,
            buffer_frames: None,
            latency_frames: None,
            safety_offset_frames: None,
        })
    }
}

pub struct Session {
    streams: Vec<Stream>,
    route: Option<routing::RouteGuard>,
    _lease: File,
    updates: Arc<ArrayQueue<Update>>,
    metrics: Arc<Metrics>,
    store: Store,
    pub input_name: String,
    pub output_name: String,
    output_profile_key: String,
    output_identity: device_identity::Identity,
    output_binding_source: String,
    output_binding_revision: u64,
    output_buffer_frames: Option<u32>,
    output_latency_frames: Option<u32>,
    output_safety_offset_frames: Option<u32>,
    rebind_count: u64,
    last_rebind_ms: Option<u64>,
    pub sample_rate: u32,
    sent_revision: u64,
    sent_music_revision: Option<u64>,
    sent_capability: crate::devices::capability::Capability,
    last_status: Instant,
    completion: Option<Instant>,
    analysis: AnalysisWorker,
    context: music_context::Worker,
    voice: Option<voice::VoiceWorker>,
    #[cfg(unix)]
    pulse: Option<pulse::Native>,
    #[cfg(target_os = "windows")]
    windows: Option<windows::Native>,
    session_id: String,
    stop_requested: bool,
    device_match_started: bool,
    last_control_result: Option<Value>,
    #[cfg(target_os = "macos")]
    tap: Option<tap::TapCapture>,
    #[cfg(target_os = "macos")]
    system_output: Option<String>,
    #[cfg(target_os = "macos")]
    system_processes: Vec<u32>,
    #[cfg(target_os = "macos")]
    system_process_pids: Vec<i32>,
    #[cfg(target_os = "macos")]
    system_device_id: u32,
    #[cfg(target_os = "macos")]
    last_device_check: Instant,
}
impl Session {
    pub fn live(store: Store, input: &str, output: Option<&str>) -> Result<Self> {
        Self::live_impl(store, input, output, false)
    }
    pub fn live_voice(store: Store, input: &str, output: Option<&str>) -> Result<Self> {
        ensure!(
            cfg!(feature = "neural"),
            "RNNoise is not compiled in; rebuild with the neural feature"
        );
        Self::live_impl(store, input, output, true)
    }
    fn live_impl(store: Store, input: &str, output: Option<&str>, neural: bool) -> Result<Self> {
        let lease = store.session_lock()?;
        let input_device = devices::select(Some(input), true)?;
        let output_device = devices::select(output, false)?;
        let input_name = input_device.description()?.name().to_owned();
        let output_name = output_device.description()?.name().to_owned();
        ensure!(
            input_name != output_name,
            "Input and output must be distinct endpoints to prevent feedback"
        );
        ensure!(
            !devices::is_virtual(&output_name),
            "Choose physical headphones or speakers as Maris output, not a loopback device"
        );
        let configurations = [48000, 44100, 96000, 88200].into_iter().find_map(|rate| {
            if neural && rate != 48000 { return None; }
            Some((devices::config(&input_device, true, rate).ok()?, devices::config(&output_device, false, rate).ok()?))
        }).context("No common mono/stereo sample rate. Set both endpoints to 48 kHz in your system audio settings")?;
        let rate = configurations.1.sample_rate();
        let state = store.load()?;
        let binding = output_binding(&store, output, &output_name, &output_device)?;
        let listening = crate::tuning::preferences::load(&store)?;
        let capability =
            crate::devices::capability::effective(&store, &binding.resolution.profile_key)?;
        let effective_music = crate::devices::capability::apply_constraints(
            listening.effective(&binding.resolution.profile_key),
            &capability,
        );
        let settings =
            Settings::compile(&state.profile, rate)?.with_music(&effective_music, rate)?;
        let analysis = AnalysisWorker::start(rate)?;
        let context = music_context::Worker::start()?;
        let metrics = Arc::new(Metrics {
            analysis_dropped: analysis.dropped.clone(),
            ..Metrics::default()
        });
        let _ = metrics.analysis_queue.set(analysis.queue.clone());
        metrics.revision.store(state.revision, Ordering::Relaxed);
        metrics
            .music_revision
            .store(listening.revision, Ordering::Relaxed);
        let updates = Arc::new(ArrayQueue::new(8));
        let queue = Arc::new(ArrayQueue::new((rate / 4) as usize));
        let capture = bridge::input(
            &input_device,
            &configurations.0,
            queue.clone(),
            metrics.clone(),
        )?;
        let voice = if neural {
            Some(voice::VoiceWorker::start(queue.clone(), rate)?)
        } else {
            None
        };
        let playback_queue = voice
            .as_ref()
            .map_or_else(|| queue.clone(), |worker| worker.output.clone());
        let playback = bridge::output(
            &output_device,
            &configurations.1,
            bridge::Source::Live(bridge::LiveSource::new(playback_queue, rate)),
            settings,
            updates.clone(),
            metrics.clone(),
        )?;
        capture.play()?;
        playback.play()?;
        Ok(Self {
            streams: vec![capture, playback],
            route: None,
            _lease: lease,
            updates,
            metrics,
            store,
            input_name,
            output_name,
            output_profile_key: binding.resolution.profile_key,
            output_identity: binding.resolution.identity,
            output_binding_source: binding.resolution.binding_source,
            output_binding_revision: binding.resolution.binding_revision,
            output_buffer_frames: binding.buffer_frames,
            output_latency_frames: binding.latency_frames,
            output_safety_offset_frames: binding.safety_offset_frames,
            rebind_count: 0,
            last_rebind_ms: None,
            sample_rate: rate,
            sent_revision: state.revision,
            last_status: Instant::now() - Duration::from_secs(1),
            completion: None,
            analysis,
            context,
            voice,
            #[cfg(unix)]
            pulse: None,
            #[cfg(target_os = "windows")]
            windows: None,
            sent_music_revision: Some(listening.revision),
            sent_capability: capability,
            session_id: new_session_id(),
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
    pub fn play(store: Store, path: &Path, output: Option<&str>, repeat: bool) -> Result<Self> {
        let lease = store.session_lock()?;
        let audio = offline::read_wav(path)?;
        ensure!(!audio.frames.is_empty(), "WAV contains no audio frames");
        let rate = audio.rate;
        let device = devices::select(output, false)?;
        let output_name = device.description()?.name().to_owned();
        ensure!(
            !devices::is_virtual(&output_name),
            "Select a physical output device for WAV playback"
        );
        let config = devices::config(&device, false, rate)?;
        let state = store.load()?;
        let binding = output_binding(&store, output, &output_name, &device)?;
        let listening = crate::tuning::preferences::load(&store)?;
        let capability =
            crate::devices::capability::effective(&store, &binding.resolution.profile_key)?;
        let effective_music = crate::devices::capability::apply_constraints(
            listening.effective(&binding.resolution.profile_key),
            &capability,
        );
        let settings =
            Settings::compile(&state.profile, rate)?.with_music(&effective_music, rate)?;
        let analysis = AnalysisWorker::start(rate)?;
        let context = music_context::Worker::start()?;
        let updates = Arc::new(ArrayQueue::new(8));
        let metrics = Arc::new(Metrics {
            analysis_dropped: analysis.dropped.clone(),
            ..Metrics::default()
        });
        let _ = metrics.analysis_queue.set(analysis.queue.clone());
        metrics.revision.store(state.revision, Ordering::Relaxed);
        metrics
            .music_revision
            .store(listening.revision, Ordering::Relaxed);
        let stream = bridge::output(
            &device,
            &config,
            bridge::Source::Wav {
                audio,
                cursor: 0,
                repeat,
            },
            settings,
            updates.clone(),
            metrics.clone(),
        )?;
        stream.play()?;
        Ok(Self {
            streams: vec![stream],
            route: None,
            _lease: lease,
            updates,
            metrics,
            store,
            input_name: path.display().to_string(),
            output_name,
            output_profile_key: binding.resolution.profile_key,
            output_identity: binding.resolution.identity,
            output_binding_source: binding.resolution.binding_source,
            output_binding_revision: binding.resolution.binding_revision,
            output_buffer_frames: binding.buffer_frames,
            output_latency_frames: binding.latency_frames,
            output_safety_offset_frames: binding.safety_offset_frames,
            rebind_count: 0,
            last_rebind_ms: None,
            sample_rate: rate,
            sent_revision: state.revision,
            last_status: Instant::now() - Duration::from_secs(1),
            completion: None,
            analysis,
            context,
            voice: None,
            #[cfg(unix)]
            pulse: None,
            #[cfg(target_os = "windows")]
            windows: None,
            sent_music_revision: Some(listening.revision),
            sent_capability: capability,
            session_id: new_session_id(),
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
    pub fn system(store: Store, output: Option<&str>) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            Self::native_system(store, output)
        }
        #[cfg(target_os = "linux")]
        {
            Self::native_pulse(store, &[], output)
        }
        #[cfg(target_os = "windows")]
        {
            Self::native_windows(store, &[], output)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (store, output);
            bail!("Automatic system-audio processing is unavailable on this platform")
        }
    }
    pub fn application_audio(store: Store, pids: &[i32], output: Option<&str>) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            Self::native_applications(store, pids, output)
        }
        #[cfg(target_os = "linux")]
        {
            ensure!(!pids.is_empty(), "Select at least one application PID");
            Self::native_pulse(store, pids, output)
        }
        #[cfg(target_os = "windows")]
        {
            ensure!(!pids.is_empty(), "Select at least one application PID");
            Self::native_windows(store, pids, output)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (store, pids, output);
            bail!("Native per-application audio processing is unavailable on this platform")
        }
    }
    /// Compatibility backend for explicit legacy callers, never used by normal application startup.
    pub fn legacy_system(store: Store, output: Option<&str>) -> Result<Self> {
        let plan = routing::plan(output)?;
        let mut session = Self::live(store.clone(), &plan.input, Some(&plan.output))?;
        session.route = Some(routing::activate(&store, &plan)?);
        Ok(session)
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn tick(&mut self) -> Result<()> {
        self.handle_control_command()?;
        if self.stop_requested {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        self.refresh_system_output()?;
        #[cfg(unix)]
        self.refresh_pulse()?;
        #[cfg(target_os = "windows")]
        self.refresh_windows_output()?;
        #[cfg(target_os = "windows")]
        ensure!(
            self.windows.as_ref().is_none_or(windows::Native::healthy),
            "Windows loopback capture stopped"
        );
        ensure!(
            self.voice.as_ref().is_none_or(voice::VoiceWorker::healthy),
            "Neural voice worker failed; stopping audio"
        );
        ensure!(
            self.metrics.errors.load(Ordering::Relaxed) == 0,
            "Audio device disconnected or stream failed; the session is stopping"
        );
        if !self.device_match_started {
            self.device_match_started = true;
            let store = self.store.clone();
            let output = self.output_name.clone();
            let profile_key = self.output_profile_key.clone();
            let existing_correction = crate::tuning::preferences::load(&store)?
                .effective(&profile_key)
                .correction_source
                .is_some();
            if !existing_correction
                && !output.to_ascii_lowercase().contains("speaker")
                && !output.eq_ignore_ascii_case("Headphones")
            {
                let _ = std::thread::Builder::new()
                    .name("maris-device-match".into())
                    .spawn(move || {
                        if let Ok(matched) = crate::devices::autoeq::resolve(&output, &store) {
                            let _ = crate::devices::capability::remember_match(
                                &store,
                                &profile_key,
                                &matched,
                                false,
                            );
                            if matched.auto_apply {
                                if let Some(entry) = matched.candidate.as_ref() {
                                    let _ = crate::devices::autoeq::apply_entry(
                                        &store,
                                        &profile_key,
                                        entry,
                                    );
                                }
                            }
                        }
                    });
            }
        }
        if let Some(window) = self.analysis.take_music_window() {
            self.context.submit(window);
        }
        session_control::queue_settings(
            &self.store,
            &self.output_profile_key,
            self.sample_rate,
            &self.updates,
            &mut self.sent_revision,
            &mut self.sent_music_revision,
            &mut self.sent_capability,
        )?;
        if self.metrics.finished.load(Ordering::Relaxed) && self.completion.is_none() {
            self.completion = Some(Instant::now());
        }
        if self.last_status.elapsed() >= Duration::from_millis(80) {
            self.store.write_json("runtime.json", &self.status())?;
            self.last_status = Instant::now();
        }
        Ok(())
    }
    pub fn finished(&self) -> bool {
        self.stop_requested
            || self
                .completion
                .is_some_and(|time| time.elapsed() > Duration::from_millis(250))
    }
    #[cfg(target_os = "macos")]
    fn system_backend(&self) -> &'static str {
        if self.pulse.is_some() {
            "pulse_server"
        } else if self.tap.is_some() {
            "coreaudio_process_tap"
        } else {
            "device_stream"
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn system_backend(&self) -> &'static str {
        #[cfg(unix)]
        if self.pulse.is_some() {
            return "pulse_server";
        }
        #[cfg(target_os = "windows")]
        if self.windows.is_some() {
            return "wasapi_process_loopback";
        }
        "device_stream"
    }
    #[cfg(target_os = "macos")]
    fn capture_scope(&self) -> &'static str {
        if let Some(pulse) = &self.pulse {
            if pulse.pids().is_empty() {
                "system_playback_excluding_maris"
            } else {
                "selected_processes"
            }
        } else if self.tap.is_none() {
            "explicit_endpoint"
        } else if self.system_processes.is_empty() {
            "system_playback_excluding_maris"
        } else {
            "selected_processes"
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn capture_scope(&self) -> &'static str {
        #[cfg(unix)]
        if let Some(pulse) = &self.pulse {
            return if pulse.pids().is_empty() {
                "system_playback_excluding_maris"
            } else {
                "selected_processes"
            };
        }
        #[cfg(target_os = "windows")]
        if let Some(native) = &self.windows {
            return if native.pids().is_empty() {
                "system_playback_excluding_maris"
            } else {
                "selected_processes"
            };
        }
        "explicit_endpoint"
    }
    #[cfg(target_os = "macos")]
    fn output_mode(&self) -> &'static str {
        if let Some(pulse) = &self.pulse {
            if pulse.selected.is_some() {
                "pinned"
            } else {
                "follow_system_default"
            }
        } else if self.tap.is_some() {
            if self.system_output.is_some() {
                "pinned"
            } else {
                "follow_system_default"
            }
        } else {
            "explicit_endpoint"
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn output_mode(&self) -> &'static str {
        #[cfg(unix)]
        if let Some(pulse) = &self.pulse {
            return if pulse.selected.is_some() {
                "pinned"
            } else {
                "follow_system_default"
            };
        }
        #[cfg(target_os = "windows")]
        if let Some(native) = &self.windows {
            return if native.selected.is_some() {
                "pinned"
            } else {
                "follow_system_default"
            };
        }
        "explicit_endpoint"
    }
    pub fn status(&self) -> Value {
        let peak = f32::from_bits(self.metrics.peak.load(Ordering::Relaxed)).max(1e-12);
        let reported_latency_frames = self
            .output_buffer_frames
            .unwrap_or(0)
            .saturating_add(self.output_latency_frames.unwrap_or(0))
            .saturating_add(self.output_safety_offset_frames.unwrap_or(0));
        let reported_latency_ms = if reported_latency_frames > 0 {
            Some(reported_latency_frames as f64 * 1000.0 / self.sample_rate as f64)
        } else {
            None
        };
        let callback_frames = self.metrics.callback_frames.load(Ordering::Relaxed);
        let callback_nanos = self
            .metrics
            .callback_processing_nanos
            .load(Ordering::Relaxed);
        let callback_average_percent = if callback_frames > 0 {
            Some(
                callback_nanos as f64 * self.sample_rate as f64 * 100.0
                    / (callback_frames as f64 * 1_000_000_000.0),
            )
        } else {
            None
        };
        let performance = json!({
            "analysis_queue_depth":self.metrics.analysis_queue.get().map_or(0,|queue|queue.len()),
            "update_queue_depth":self.updates.len(),
            "callback_processing_average_percent":callback_average_percent,
            "callback_processing_max_us":self.metrics.callback_max_nanos.load(Ordering::Relaxed) as f64 / 1000.0,
            "callback_calls":self.metrics.callback_calls.load(Ordering::Relaxed),
            "callback_frames":callback_frames,
            "worker_cpu_percent":Value::Null,
            "worker_processing_nanos":self.metrics.worker_processing_nanos.load(Ordering::Relaxed),
            "worker_frames":self.metrics.worker_frames.load(Ordering::Relaxed),
            "model_latency_ms":self.context.latest().map(|context|context.inference_ms)
        });
        let mut status = json!({"active":!self.stop_requested,"session_id":self.session_id,"pid":std::process::id(),
        "analysis":self.analysis.latest(),"visualization":self.analysis.visual(),
        "music_context":self.context.latest(),"music_context_status":self.context.status(),
        "engine_revision":crate::dsp::DSP_REVISION,
        "executable":std::env::current_exe().ok(),
        "effective_preamp_db":f32::from_bits(self.metrics.effective_gain.load(Ordering::Relaxed)),
        "adaptive_reduction_db":f32::from_bits(self.metrics.adaptive_reduction.load(Ordering::Relaxed)),
        "clock_bridge":if self.system_backend() == "pulse_server" { "server_resampling" } else { "windowed_sinc_128" },"neural":self.voice.as_ref().map(voice::VoiceWorker::status),
        "captured_frames":self.metrics.captured_frames.load(Ordering::Relaxed),
        "system_backend":self.system_backend(),
        "capture_scope":self.capture_scope(),
        "output_mode":self.output_mode(),
        "peak_left_dbfs":20.0*(f32::from_bits(self.metrics.peak_left.load(Ordering::Relaxed)).max(1e-12) as f64).log10(),
        "peak_right_dbfs":20.0*(f32::from_bits(self.metrics.peak_right.load(Ordering::Relaxed)).max(1e-12) as f64).log10(),"updated_at_ms":now_ms(),"input":self.input_name,"output":self.output_name,
        "sample_rate":self.sample_rate,"peak_dbfs":20.0 * (peak as f64).log10(),"frames":self.metrics.frames.load(Ordering::Relaxed),
        "underruns":self.metrics.underruns.load(Ordering::Relaxed),"overruns":self.metrics.overruns.load(Ordering::Relaxed),
        "applied_music_revision":self.metrics.music_revision.load(Ordering::Relaxed),
        "music_processing":true,"tonal_bypass":self.metrics.tonal_bypass.load(Ordering::Relaxed),
        "applied_revision":self.metrics.revision.load(Ordering::Relaxed),"sample_peak_limiter_dbfs":-1.0,
        "performance":performance});
        status["settings_pending"] = json!(!self.updates.is_empty());
        status["continuity"] = self.metrics.continuity();
        status["source_started"] = json!(self.metrics.source_started.load(Ordering::Relaxed));
        status["output_faded_out"] = json!(self.metrics.faded_out.load(Ordering::Acquire));
        status["profile_key"] = json!(&self.output_profile_key);
        status["device_identity"] = json!(&self.output_identity);
        status["profile_binding_source"] = json!(&self.output_binding_source);
        status["device_binding_revision"] = json!(self.output_binding_revision);
        status["rebind_count"] = json!(self.rebind_count);
        status["last_rebind_ms"] = json!(self.last_rebind_ms);
        status["reported_output_buffer_frames"] = json!(self.output_buffer_frames);
        status["reported_output_latency_frames"] = json!(self.output_latency_frames);
        status["reported_output_safety_offset_frames"] = json!(self.output_safety_offset_frames);
        status["reported_output_path_latency_ms"] = json!(reported_latency_ms);
        status["last_control_result"] = json!(&self.last_control_result);
        #[cfg(target_os = "macos")]
        {
            status["captured_process_objects"] = json!(&self.system_processes);
            status["captured_application_pids"] = json!(&self.system_process_pids);
        }
        #[cfg(unix)]
        if let Some(pulse) = &self.pulse {
            status["captured_application_pids"] = json!(pulse.pids());
            status["processing_executor"] = json!("local_pcm_worker");
        }
        #[cfg(target_os = "windows")]
        if let Some(native) = &self.windows {
            status["captured_application_pids"] = json!(native.pids());
            status["processing_executor"] = json!("wasapi_capture_worker");
            let capture = native.capture_metrics();
            status["captured_frames"] = json!(capture.captured_frames.load(Ordering::Relaxed));
            status["overruns"] = json!(capture.overruns.load(Ordering::Relaxed));
            status["capture_continuity"] = capture.continuity();
            status["performance"]["worker_processing_nanos"] =
                json!(capture.worker_processing_nanos.load(Ordering::Relaxed));
            status["performance"]["worker_frames"] =
                json!(capture.worker_frames.load(Ordering::Relaxed));
        }
        status
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.metrics.stopping.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(40));
        self.streams.clear();
        #[cfg(unix)]
        self.pulse.take();
        #[cfg(target_os = "windows")]
        self.windows.take();
        #[cfg(target_os = "macos")]
        self.tap.take();
        self.voice.take();
        self.route.take();
        let _ = self.store.write_json(
            "runtime.json",
            &json!({"active":false,"updated_at_ms":now_ms()}),
        );
    }
}
fn new_session_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn now_ms() -> u64 {
    crate::analysis::now_ms()
}
pub fn runtime_status(store: &Store) -> Value {
    runtime_status_at(store, now_ms())
}

pub(crate) fn runtime_status_at(store: &Store, now: u64) -> Value {
    let mut status = read_json::<Value>(&store.directory.join("runtime.json"))
        .unwrap_or_else(|_| json!({"active":false}));
    if !status.is_object() {
        status = json!({"active":false});
    }
    let stale = status
        .get("updated_at_ms")
        .and_then(Value::as_u64)
        .and_then(|time| now.checked_sub(time))
        .is_none_or(|age| age > 3000);
    if stale {
        status["active"] = json!(false);
        status["stale"] = json!(true);
    }
    status
}
pub fn restore(store: &Store) -> Result<()> {
    let _lease = store.session_lock()?;
    #[cfg(target_os = "linux")]
    {
        pulse::recover(store)
    }
    #[cfg(not(target_os = "linux"))]
    {
        routing::restore(store)
    }
}
#[cfg(unix)]
pub fn watch_route(store: &Store, token: &str) -> Result<()> {
    pulse::watch(store, token)
}
pub fn native_controls(state: &Value) -> bool {
    matches!(
        state["system_backend"].as_str(),
        Some("coreaudio_process_tap" | "pulse_server" | "wasapi_process_loopback")
    )
}
pub fn doctor(store: &Store) -> Result<Value> {
    let list = devices()?;
    #[cfg(target_os = "macos")]
    let native = json!({"available":tap_ffi::TapApi::load().is_ok(),"backend":"coreaudio_process_tap","driver_required":false,"microphone_required":false,"default_output":tap_ffi::default_output().and_then(tap_ffi::device_name).ok()});
    #[cfg(target_os = "linux")]
    let native = pulse::doctor();
    #[cfg(target_os = "windows")]
    let native = windows::doctor();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let native = json!({"available":false,"reason":"This operating system is outside Maris' supported macOS, Linux and Windows targets"});
    let virtual_inputs: Vec<_> = list
        .iter()
        .filter(|d| d.direction == "input" && devices::is_virtual(&d.name))
        .map(|d| &d.name)
        .collect();
    let setup = match std::env::consts::OS {
        "macos" => "Open Maris and allow system audio recording in the macOS permission dialog. A private native CoreAudio tap automatically captures system playback, excludes Maris to prevent feedback, and feeds the current output through DSP. No BlackHole, microphone, or manual routing is required. macOS 14.2+ is required.",
        "windows" => "Start with maris system --accept-routing. Native WASAPI loopback captures system playback and renders the processed path to the selected physical output. While a route is active, Maris temporarily mutes only the original render sessions it replaces and restores each prior mute state on stop; it does not change the Windows default output or session volume scalar.",
        "linux" => "Start with maris system --accept-routing. A local PulseAudio or PipeWire pulse server and pulseaudio-utils are required. Maris temporarily redirects pulse application streams through its monitor, DSP and selected output, then restores their routes. No OS default or volume is changed. Native PipeWire-only clients are not captured by this backend.",
        _ => "This platform is not a supported target.",
    };
    if !["macos", "windows", "linux"].contains(&std::env::consts::OS) {
        bail!("Unsupported operating system");
    }
    Ok(
        json!({"platform":std::env::consts::OS,"platform_capabilities":crate::audio::platform::capabilities(),"devices":list,"virtual_inputs":virtual_inputs,"setup":setup,
        "state_directory":store.directory,"capture_started":false,"automatic_system_routing":native["available"],"native_system_audio":native,
        "note":"WAV playback and rendering do not require a virtual device. Start with low physical volume. Digital dBFS is not acoustic dB SPL."}),
    )
}
