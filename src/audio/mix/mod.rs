//! Explicit multi-input, two-output sessions. Never changes OS volume or default devices.
mod render;
use super::{bridge, devices, output_binding, Metrics, OutputBinding, Update};
use crate::{
    analysis,
    control::store::Store,
    dsp::Settings,
    mixer::{self, MixerConfig, BUS_COUNT},
};
use anyhow::{ensure, Context, Result};
use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, FromSample, SampleFormat, SizedSample, Stream, SupportedStreamConfig,
};
use crossbeam_queue::ArrayQueue;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::File,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub fn validate_routes(config: &MixerConfig) -> Result<()> {
    config.validate()?;
    let inputs: Vec<_> = config
        .strips
        .iter()
        .filter_map(|s| s.source_device.as_deref())
        .collect();
    let outputs: Vec<_> = config
        .buses
        .iter()
        .filter_map(|b| b.output_device.as_deref())
        .collect();
    ensure!(
        !inputs.is_empty(),
        "Assign at least one mixer input before starting"
    );
    ensure!(
        !outputs.is_empty(),
        "Assign at least one mixer output before starting"
    );
    ensure!(
        inputs.iter().collect::<BTreeSet<_>>().len() == inputs.len(),
        "A mixer input may be captured only once"
    );
    ensure!(
        outputs.iter().collect::<BTreeSet<_>>().len() == outputs.len(),
        "Mixer buses require distinct output devices"
    );
    for source in inputs {
        if let Some(text) = source.strip_prefix("pid:") {
            let pid: i32 = text.parse().context("Invalid mixer application PID")?;
            ensure!(
                text == pid.to_string(),
                "Use the canonical pid:N mixer selector"
            );
            ensure!(
                pid > 0 && pid != std::process::id() as i32,
                "Invalid mixer application PID"
            );
            ensure!(
                cfg!(any(
                    target_os = "macos",
                    target_os = "linux",
                    target_os = "windows"
                )),
                "Independent application strips require macOS, Linux or Windows"
            );
        }
    }
    Ok(())
}
pub fn same_routes(a: &MixerConfig, b: &MixerConfig) -> bool {
    a.strips.len() == b.strips.len()
        && a.strips.iter().zip(&b.strips).all(|(a, b)| {
            a.id == b.id
                && a.source_device == b.source_device
                && a.speech_denoise == b.speech_denoise
        })
        && a.buses.iter().zip(&b.buses).all(|(a, b)| {
            a.id == b.id
                && a.output_device == b.output_device
                && (a.delay_ms - b.delay_ms).abs() < f64::EPSILON
        })
}

enum Capture {
    Device(Stream),
    #[cfg(target_os = "macos")]
    ApplicationMac {
        tap: super::tap::TapCapture,
        object: u32,
        pid: i32,
    },
    #[cfg(target_os = "linux")]
    ApplicationPulse(Box<super::pulse::ApplicationCapture>),
    #[cfg(target_os = "windows")]
    ApplicationWindows(super::windows::ApplicationCapture),
}
impl Capture {
    fn start(&mut self) -> Result<()> {
        match self {
            Self::Device(stream) => Ok(stream.play()?),
            #[cfg(target_os = "macos")]
            Self::ApplicationMac { tap, .. } => tap.start(),
            #[cfg(target_os = "linux")]
            Self::ApplicationPulse(capture) => capture.start(),
            #[cfg(target_os = "windows")]
            Self::ApplicationWindows(capture) => capture.start(),
        }
    }
    fn check(&mut self) -> Result<()> {
        match self {
            Self::Device(_) => Ok(()),
            #[cfg(target_os = "macos")]
            Self::ApplicationMac { tap, object, pid } => {
                ensure!(
                    super::tap_ffi::process_object_for_pid(*pid).is_ok_and(|now| now == *object),
                    "A captured application exited or changed identity"
                );
                ensure!(
                    !tap.needs_rebuild()?,
                    "Application capture format changed; stop and restart the mixer"
                );
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Self::ApplicationPulse(capture) => capture.check(),
            #[cfg(target_os = "windows")]
            Self::ApplicationWindows(capture) => capture.check(),
        }
    }
}
struct Output {
    device: Device,
    format: SupportedStreamConfig,
    binding: OutputBinding,
    settings: Settings,
    updates: Arc<ArrayQueue<Update>>,
    revisions: (u64, u64),
    capability: crate::devices::capability::Capability,
}
fn output_settings(
    store: &Store,
    binding: &OutputBinding,
    rate: u32,
    state: &crate::control::store::Snapshot,
    library: &crate::tuning::preferences::Library,
) -> Result<(Settings, crate::devices::capability::Capability)> {
    let key = &binding.resolution.profile_key;
    let capability = crate::devices::capability::effective(store, key)?;
    let profile =
        crate::devices::capability::apply_constraints(library.effective(key), &capability);
    Ok((
        Settings::compile(&state.profile, rate)?.with_music(&profile, rate)?,
        capability,
    ))
}

pub struct Session {
    captures: Vec<Capture>,
    streams: Vec<Stream>,
    outputs: [Option<Output>; BUS_COUNT],
    output_metrics: [Arc<Metrics>; BUS_COUNT],
    input_metrics: Vec<Arc<Metrics>>,
    input_rates: Vec<Option<u32>>,
    input_peaks: Vec<Option<Arc<AtomicU64>>>,
    voice_workers: Vec<Option<super::voice::VoiceWorker>>,
    queue: Arc<ArrayQueue<render::Update>>,
    revision: Arc<AtomicU64>,
    topology: MixerConfig,
    sent: u64,
    store: Store,
    rate: u32,
    primary: usize,
    id: String,
    notice: Option<String>,
    last_frames: [u64; BUS_COUNT],
    progress: [Instant; BUS_COUNT],
    stop: bool,
    analysis: analysis::AnalysisWorker,
    context: crate::analysis::context::Worker,
    _lease: File,
}
impl Session {
    pub fn start(store: Store, rate: u32, authorized: bool) -> Result<Self> {
        ensure!(authorized, "Mixer capture requires --accept-routing");
        ensure!(
            (44100..=192000).contains(&rate),
            "Mixer sample rate must be 44100..192000 Hz"
        );
        let state = mixer::load(&store)?;
        validate_routes(&state.config)?;
        let initial = render::Update::compile(&state.config, state.revision, rate)?;
        let lease = store.session_lock()?;
        #[cfg(target_os = "linux")]
        super::pulse::recover(&store)?;
        let mut outputs: [Option<Output>; BUS_COUNT] = std::array::from_fn(|_| None);
        let primary = state
            .config
            .buses
            .iter()
            .position(|b| b.output_device.is_some())
            .context("Missing mixer output")?;
        let analysis = analysis::AnalysisWorker::start(rate)?;
        let context = crate::analysis::context::Worker::start()?;
        let output_metrics: [Arc<Metrics>; BUS_COUNT] = std::array::from_fn(|index| {
            Arc::new(Metrics {
                analysis_dropped: if index == primary {
                    analysis.dropped.clone()
                } else {
                    Arc::new(AtomicU64::new(0))
                },
                ..Metrics::default()
            })
        });
        let _ = output_metrics[primary]
            .analysis_queue
            .set(analysis.queue.clone());
        let eq = store.load()?;
        let listening = crate::tuning::preferences::load(&store)?;
        let mut identities = BTreeSet::new();
        // Select and compile every output before preparing any capture resource.
        for (bus, setting) in state.config.buses.iter().enumerate() {
            if let Some(selector) = setting.output_device.as_deref() {
                let device = devices::select(Some(selector), false)?;
                let name = device.description()?.name().to_owned();
                let binding = output_binding(&store, Some(selector), &name, &device)?;
                ensure!(
                    identities.insert(binding.resolution.profile_key.clone()),
                    "Mixer outputs resolve to the same device"
                );
                let format = if bus == primary {
                    devices::config(&device, false, rate)?
                } else {
                    devices::config_near(&device, false, rate)?
                };
                let (settings, capability) =
                    output_settings(&store, &binding, format.sample_rate(), &eq, &listening)?;
                let (eq_revision, music_revision) = (eq.revision, listening.revision);
                let updates = Arc::new(ArrayQueue::new(8));
                updates
                    .push(Update {
                        settings,
                        revision: eq_revision,
                        music_revision,
                    })
                    .ok();
                outputs[bus] = Some(Output {
                    device,
                    format,
                    binding,
                    settings,
                    updates,
                    revisions: (eq_revision, music_revision),
                    capability,
                });
            }
        }
        #[cfg(target_os = "windows")]
        {
            let pids = state
                .config
                .strips
                .iter()
                .filter_map(|strip| strip.source_device.as_deref()?.strip_prefix("pid:"))
                .map(str::parse::<i32>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            // Separate strips must not capture the same process tree twice.
            super::windows_route::validate_capture_pids(&pids)?;
        }
        let mut captures = Vec::new();
        let mut inputs = Vec::new();
        let mut input_metrics = Vec::new();
        let mut input_rates = Vec::new();
        let mut input_peaks = Vec::new();
        let mut voice_workers = Vec::new();
        let mut source_names = BTreeSet::new();
        for strip in &state.config.strips {
            let metrics = Arc::new(Metrics::default());
            if let Some(selector) = strip.source_device.as_deref() {
                let queue = Arc::new(ArrayQueue::new(48_000));
                let input_rate;
                if let Some(pid) = selector.strip_prefix("pid:") {
                    #[cfg(target_os = "macos")]
                    {
                        let pid: i32 = pid.parse()?;
                        let object = super::tap_ffi::process_object_for_pid(pid)?;
                        let tap = super::tap::TapCapture::prepare_processes(
                            queue.clone(),
                            metrics.clone(),
                            &[object],
                        )?;
                        input_rate = tap.rate;
                        captures.push(Capture::ApplicationMac { tap, object, pid });
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let pid: i32 = pid.parse()?;
                        let capture = super::pulse::ApplicationCapture::prepare(
                            &store,
                            pid,
                            queue.clone(),
                            metrics.clone(),
                        )?;
                        input_rate = 48_000;
                        captures.push(Capture::ApplicationPulse(Box::new(capture)));
                    }
                    #[cfg(target_os = "windows")]
                    {
                        let pid: i32 = pid.parse()?;
                        let capture = super::windows::ApplicationCapture::prepare(
                            pid,
                            queue.clone(),
                            metrics.clone(),
                        )?;
                        input_rate = 48_000;
                        captures.push(Capture::ApplicationWindows(capture));
                    }
                    #[cfg(not(any(
                        target_os = "macos",
                        target_os = "linux",
                        target_os = "windows"
                    )))]
                    anyhow::bail!(
                        "Independent application tap {pid} is unavailable on this platform"
                    );
                } else {
                    let device = devices::select(Some(selector), true)?;
                    let name = device.description()?.name().to_owned();
                    ensure!(
                        source_names.insert(name),
                        "Mixer input selectors resolve to the same or ambiguously named device"
                    );
                    let format = devices::config_near(&device, true, rate)?;
                    input_rate = format.sample_rate();
                    captures.push(Capture::Device(bridge::input(
                        &device,
                        &format,
                        queue.clone(),
                        metrics.clone(),
                    )?));
                }
                let (source_queue, voice_worker) = if strip.speech_denoise {
                    ensure!(
                        input_rate == 48_000,
                        "Speech denoise requires a 48 kHz mixer source"
                    );
                    let worker = super::voice::VoiceWorker::start(queue.clone(), input_rate)?;
                    (worker.output.clone(), Some(worker))
                } else {
                    (queue, None)
                };
                let input =
                    render::Input::with_rates(source_queue, input_rate, rate, metrics.clone())?;
                input_rates.push(Some(input_rate));
                input_peaks.push(Some(input.peak.clone()));
                inputs.push(Some(input));
                voice_workers.push(voice_worker);
            } else {
                inputs.push(None);
                input_peaks.push(None);
                input_rates.push(None);
                voice_workers.push(None);
            }
            input_metrics.push(metrics);
        }
        let mut streams = Vec::new();
        let mut sends: [Option<Arc<ArrayQueue<[f32; 2]>>>; BUS_COUNT] =
            std::array::from_fn(|_| None);
        for (bus, output) in outputs.iter().enumerate() {
            if bus == primary {
                continue;
            }
            if let Some(output) = output {
                let queue = Arc::new(ArrayQueue::new(rate as usize / 4));
                streams.push(bridge::output(
                    &output.device,
                    &output.format,
                    bridge::Source::Live(bridge::LiveSource::with_rates(
                        queue.clone(),
                        rate,
                        output.format.sample_rate(),
                    )?),
                    output.settings,
                    output.updates.clone(),
                    output_metrics[bus].clone(),
                )?);
                sends[bus] = Some(queue);
            }
        }
        let queue = Arc::new(ArrayQueue::new(2));
        queue.push(initial).ok();
        let revision = Arc::new(AtomicU64::new(u64::MAX));
        let graph = render::Graph::new(render::GraphConfig {
            inputs,
            initial,
            delay_ms: [
                state.config.buses[0].delay_ms,
                state.config.buses[1].delay_ms,
            ],
            rate,
            primary,
            sends,
            send_metrics: output_metrics.clone(),
            updates: queue.clone(),
            revision: revision.clone(),
        })?;
        let output = outputs[primary]
            .as_ref()
            .context("Missing primary mixer output")?;
        streams.push(master_stream(
            output,
            graph,
            output_metrics[primary].clone(),
        )?);
        // The session owns all resources before the first play call, so any startup failure tears them down.
        let mut result = Self {
            captures,
            streams,
            outputs,
            output_metrics,
            input_metrics,
            input_rates,
            input_peaks,
            voice_workers,
            queue,
            revision,
            topology: state.config,
            sent: state.revision,
            store,
            rate,
            primary,
            id: super::new_session_id(),
            notice: None,
            last_frames: [0; BUS_COUNT],
            progress: [Instant::now(); BUS_COUNT],
            stop: false,
            analysis,
            context,
            _lease: lease,
        };
        for stream in &result.streams {
            stream.play()?;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while result.outputs.iter().enumerate().any(|(bus, output)| {
            output.is_some()
                && result.output_metrics[bus]
                    .callback_calls
                    .load(Ordering::Acquire)
                    == 0
        }) {
            ensure!(
                Instant::now() < deadline,
                "Mixer output did not become ready; application capture was not started"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        ensure!(
            mixer::load(&result.store)?.revision == result.sent,
            "Mixer changed during startup; confirm the current configuration again"
        );
        for capture in &mut result.captures {
            capture.start()?;
        }
        result.publish(true)?;
        Ok(result)
    }
    pub fn tick(&mut self) -> Result<bool> {
        match take_mixer_stop(&self.store, &self.id) {
            Ok(stop) => self.stop |= stop,
            Err(error) => self.notice = Some(format!("Invalid mixer command: {error:#}")),
        }
        if self.stop {
            return Ok(false);
        }
        for capture in &mut self.captures {
            capture.check()?;
        }
        for (bus, output) in self.outputs.iter().enumerate() {
            if output.is_none() {
                continue;
            }
            let metrics = &self.output_metrics[bus];
            ensure!(
                metrics.errors.load(Ordering::Relaxed) == 0,
                "Mixer output stream failed"
            );
            let frames = metrics.frames.load(Ordering::Relaxed);
            if frames != self.last_frames[bus] {
                self.progress[bus] = Instant::now();
                self.last_frames[bus] = frames;
            }
            ensure!(
                self.progress[bus].elapsed() < Duration::from_secs(3),
                "Mixer output stopped delivering callbacks"
            );
        }
        ensure!(
            self.input_metrics
                .iter()
                .all(|m| m.errors.load(Ordering::Relaxed) == 0),
            "Mixer input stream failed"
        );
        let current = mixer::load(&self.store)?;
        if current.revision != self.sent {
            if !same_routes(&self.topology, &current.config) {
                self.notice = Some(
                    "Input/output assignments changed; stop and restart the mixer to use them"
                        .into(),
                );
            } else {
                match render::Update::compile(&current.config, current.revision, self.rate) {
                    Ok(update) => {
                        if self.queue.push(update).is_ok() {
                            self.sent = current.revision;
                            self.notice = None;
                        }
                    }
                    Err(error) => {
                        self.notice = Some(format!("Mixer settings were not applied: {error:#}"))
                    }
                }
            }
        }
        if let Err(error) = self.refresh_outputs() {
            self.notice = Some(format!("Output settings were not applied: {error:#}"));
        }
        if let Some(window) = self.analysis.take_music_window() {
            self.context.submit(window);
        }
        self.publish(true)?;
        Ok(true)
    }
    fn refresh_outputs(&mut self) -> Result<()> {
        let state = self.store.load()?;
        let library = crate::tuning::preferences::load(&self.store)?;
        let bindings = crate::devices::identity::load(&self.store)?;
        for output in self.outputs.iter_mut().flatten() {
            let key = &output.binding.resolution.profile_key;
            if let Some(uid) = &output.binding.resolution.identity.stable_id {
                ensure!(
                    bindings
                        .bindings
                        .get(uid)
                        .is_some_and(|b| &b.profile_key == key),
                    "Device profile binding changed; stop and restart the mixer"
                );
            }
            let capability = crate::devices::capability::effective(&self.store, key)?;
            if output.revisions == (state.revision, library.revision)
                && output.capability == capability
            {
                continue;
            }
            if !output.updates.is_empty() {
                continue;
            }
            let (settings, capability) = output_settings(
                &self.store,
                &output.binding,
                output.format.sample_rate(),
                &state,
                &library,
            )?;
            if output
                .updates
                .push(Update {
                    settings,
                    revision: state.revision,
                    music_revision: library.revision,
                })
                .is_ok()
            {
                output.settings = settings;
                output.capability = capability;
                output.revisions = (state.revision, library.revision);
            }
        }
        Ok(())
    }
    fn publish(&self, active: bool) -> Result<()> {
        let primary = self.outputs[self.primary]
            .as_ref()
            .context("Missing primary output")?;
        let metrics = &self.output_metrics[self.primary];
        let state = mixer::load(&self.store)?;
        let processing = self.revision.load(Ordering::Acquire);
        let db = |bits| 20.0 * f32::from_bits(bits).max(1e-6).log10();
        let buses: Vec<_> = self.outputs.iter().enumerate().filter_map(|(index, output)| output.as_ref().map(|output| {
            let m = &self.output_metrics[index];
            json!({"id":self.topology.buses[index].id,"output":output.binding.resolution.identity.display_name,
                "profile_key":output.binding.resolution.profile_key,"sample_rate":output.format.sample_rate(),
                "delay_ms":self.topology.buses[index].delay_ms,"frames":m.frames.load(Ordering::Relaxed),"continuity":m.continuity(),
                "peak_left_dbfs":db(m.peak_left.load(Ordering::Relaxed)),"peak_right_dbfs":db(m.peak_right.load(Ordering::Relaxed)),
                "applied_revision":m.revision.load(Ordering::Relaxed),"applied_music_revision":m.music_revision.load(Ordering::Relaxed),
                "underruns":m.underruns.load(Ordering::Relaxed),"overruns":m.overruns.load(Ordering::Relaxed)})
        })).collect();
        let strips: Vec<_> = self.input_metrics.iter().enumerate().map(|(index,m)| json!({"id":self.topology.strips[index].id,
            "sample_rate":self.input_rates[index],"continuity":m.continuity(),"captured_frames":m.captured_frames.load(Ordering::Relaxed),"underruns":m.underruns.load(Ordering::Relaxed),
            "overruns":m.overruns.load(Ordering::Relaxed),"speech_denoise":self.topology.strips[index].speech_denoise,
            "speech_worker":self.voice_workers[index].as_ref().map(super::voice::VoiceWorker::status),
            "peak_dbfs":self.input_peaks[index].as_ref().map(|p|20.0*f64::from_bits(p.swap(0,Ordering::Relaxed)).max(1e-6).log10())})).collect();
        let mut status = json!({"active":active,"session_id":self.id,"pid":std::process::id(),
            "system_backend":"multi_device_mixer","engine_revision":crate::dsp::DSP_REVISION,"music_processing":true,
            "output":primary.binding.resolution.identity.display_name,"profile_key":primary.binding.resolution.profile_key,
            "device_identity":primary.binding.resolution.identity,"device_binding_revision":primary.binding.resolution.binding_revision,
            "profile_binding_source":primary.binding.resolution.binding_source,"output_mode":"mixer_assignments","rebind_count":0,
            "sample_rate":self.rate,"applied_revision":metrics.revision.load(Ordering::Relaxed),"applied_music_revision":metrics.music_revision.load(Ordering::Relaxed),
            "peak_left_dbfs":db(metrics.peak_left.load(Ordering::Relaxed)),"peak_right_dbfs":db(metrics.peak_right.load(Ordering::Relaxed)),
            "effective_preamp_db":f32::from_bits(metrics.effective_gain.load(Ordering::Relaxed)),"tonal_bypass":metrics.tonal_bypass.load(Ordering::Relaxed),
            "updated_at_ms":analysis::now_ms(),"mixer":{"revision":state.revision,"processing_revision":(processing!=u64::MAX).then_some(processing),
                "restart_required":!same_routes(&self.topology,&state.config),"strips":strips,"buses":buses,"error":self.notice},
            "analysis":self.analysis.latest(),"visualization":self.analysis.visual(),"analysis_bus":self.topology.buses[self.primary].id,
            "music_context":self.context.latest(),"music_context_status":self.context.status(),
            "captured_frames":self.input_metrics.iter().map(|m|m.captured_frames.load(Ordering::Relaxed)).sum::<u64>(),
            "frames":metrics.frames.load(Ordering::Relaxed),"source_started":metrics.source_started.load(Ordering::Relaxed),
            "sample_peak_limiter_dbfs":-1.0,"clock_bridge":"windowed_sinc_128","capture_scope":"mixer_assignments",
            "underruns":self.input_metrics.iter().chain(self.output_metrics.iter()).map(|m|m.underruns.load(Ordering::Relaxed)).sum::<u64>(),
            "overruns":self.input_metrics.iter().chain(self.output_metrics.iter()).map(|m|m.overruns.load(Ordering::Relaxed)).sum::<u64>(),
            "performance":{"callback_calls":metrics.callback_calls.load(Ordering::Relaxed),"callback_frames":metrics.callback_frames.load(Ordering::Relaxed),
                "callback_processing_average_percent":(metrics.callback_frames.load(Ordering::Relaxed)>0).then(|| metrics.callback_processing_nanos.load(Ordering::Relaxed) as f64*self.rate as f64*100.0/(metrics.callback_frames.load(Ordering::Relaxed) as f64*1e9)),
                "callback_processing_max_us":metrics.callback_max_nanos.load(Ordering::Relaxed) as f64/1000.0,"worker_cpu_percent":Value::Null}});
        status["settings_pending"] = json!(self
            .outputs
            .iter()
            .flatten()
            .any(|output| !output.updates.is_empty()));
        status["continuity"] = metrics.continuity();
        self.store.write_json("runtime.json", &status)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        for m in &self.output_metrics {
            m.stopping.store(true, Ordering::Release);
        }
        let until = Instant::now() + Duration::from_millis(100);
        while Instant::now() < until
            && self.outputs.iter().enumerate().any(|(i, o)| {
                o.is_some() && !self.output_metrics[i].faded_out.load(Ordering::Acquire)
            })
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.captures.clear();
        self.streams.clear();
        let _ = self.publish(false);
    }
}
fn master_stream(output: &Output, graph: render::Graph, metrics: Arc<Metrics>) -> Result<Stream> {
    match output.format.sample_format() {
        SampleFormat::F32 => master_typed::<f32>(output, graph, metrics),
        SampleFormat::I16 => master_typed::<i16>(output, graph, metrics),
        SampleFormat::U16 => master_typed::<u16>(output, graph, metrics),
        SampleFormat::I32 => master_typed::<i32>(output, graph, metrics),
        _ => anyhow::bail!("Unsupported mixer sample format"),
    }
}
fn master_typed<T: SizedSample + FromSample<f32>>(
    output: &Output,
    mut graph: render::Graph,
    metrics: Arc<Metrics>,
) -> Result<Stream> {
    let channels = output.format.channels() as usize;
    let mut renderer = bridge::Renderer::new(
        output.settings,
        output.format.sample_rate(),
        output.updates.clone(),
        metrics.clone(),
    );
    Ok(output.device.build_output_stream(
        &output.format.config(),
        move |data: &mut [T], _| {
            graph.begin_block();
            renderer.render(data, channels, || graph.frame());
        },
        move |_| {
            metrics.errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )?)
}
fn take_mixer_stop(store: &Store, session: &str) -> Result<bool> {
    let Some(command) = crate::control::take_command(store, session)? else {
        return Ok(false);
    };
    ensure!(
        command["action"] == "stop",
        "Stop the mixer before changing its input or output assignments"
    );
    Ok(true)
}

pub fn run(store: Store, rate: u32, authorized: bool, seconds: Option<u64>) -> Result<Value> {
    ensure!(authorized, "Mixer capture requires --accept-routing");
    ensure!(seconds != Some(0), "Mixer duration must be positive");
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    let mut session = Session::start(store, rate, true)?;
    let began = Instant::now();
    while !stop.load(Ordering::Relaxed)
        && seconds.is_none_or(|s| began.elapsed() < Duration::from_secs(s))
        && session.tick()?
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(json!({"stopped":true,"session_id":session.id}))
}

#[cfg(test)]
#[path = "../../../tests/mixing/render.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/mixing/control.rs"]
mod control_tests;
