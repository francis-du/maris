use super::{Metrics, Update};
use crate::{
    audio::offline::AudioFile,
    dsp::{Processor, Settings},
};
use anyhow::Result;
use cpal::{
    traits::DeviceTrait, Device, FromSample, Sample, SampleFormat, SizedSample, Stream,
    SupportedStreamConfig,
};
use crossbeam_queue::ArrayQueue;
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

pub enum Source {
    Wav {
        audio: AudioFile,
        cursor: usize,
        repeat: bool,
    },
    Live(LiveSource),
}
pub struct LiveSource {
    bridge: crate::dsp::resample::ClockBridge,
    observed: (u64, u64, u64, u64),
    capture_metrics: Option<Arc<Metrics>>,
}
impl LiveSource {
    pub fn new(queue: Arc<ArrayQueue<[f32; 2]>>, rate: u32) -> Self {
        Self {
            bridge: crate::dsp::resample::ClockBridge::new(queue, (rate / 100) as usize),
            observed: (0, 0, 0, 0),
            capture_metrics: None,
        }
    }
    pub fn with_rates(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        input_rate: u32,
        output_rate: u32,
    ) -> Result<Self> {
        Ok(Self {
            bridge: crate::dsp::resample::ClockBridge::with_rates(
                queue,
                (input_rate / 100) as usize,
                input_rate,
                output_rate,
            )?,
            observed: (0, 0, 0, 0),
            capture_metrics: None,
        })
    }
    #[cfg(target_os = "macos")]
    pub fn system(queue: Arc<ArrayQueue<[f32; 2]>>, rate: u32) -> Self {
        Self {
            bridge: crate::dsp::resample::ClockBridge::new(queue, (rate as usize / 20).max(2048)),
            observed: (0, 0, 0, 0),
            capture_metrics: None,
        }
    }
    #[cfg(any(target_os = "windows", test))]
    pub(super) fn with_capture_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.capture_metrics = Some(metrics);
        self
    }
    pub(super) fn frame(&mut self, metrics: &Metrics) -> ([f32; 2], bool) {
        let capture = self.capture_metrics.as_deref().unwrap_or(metrics);
        let observed = (
            capture.overruns.load(Ordering::Acquire),
            capture.capture_discontinuities.load(Ordering::Acquire),
            metrics.callback_discontinuities.load(Ordering::Acquire),
            capture.configuration_events.load(Ordering::Acquire),
        );
        if self.observed != observed {
            self.bridge.reset();
            self.observed = observed;
            metrics.stream_resets.fetch_add(1, Ordering::Relaxed);
            return ([0.0; 2], false);
        }
        if observed.3 != capture.checked_configuration_events.load(Ordering::Acquire) {
            return ([0.0; 2], false);
        }
        let ready_before = self.bridge.is_ready();
        let (output, underrun) = self.bridge.next_frame();
        if underrun {
            metrics.underruns.fetch_add(1, Ordering::Relaxed);
        }
        (output, ready_before || self.bridge.is_ready())
    }
}
impl Source {
    fn frame(&mut self, metrics: &Metrics) -> ([f32; 2], bool) {
        match self {
            Source::Live(source) => source.frame(metrics),
            Source::Wav {
                audio,
                cursor,
                repeat,
            } => {
                if *cursor >= audio.frames.len() {
                    if *repeat && !audio.frames.is_empty() {
                        *cursor = 0;
                    } else {
                        metrics.finished.store(true, Ordering::Relaxed);
                        return ([0.0; 2], false);
                    }
                }
                let frame = audio.frames[*cursor];
                *cursor += 1;
                (frame, true)
            }
        }
    }
}

// The DSP and startup envelope advance on actual source frames, not on silent
// output callbacks while a tap starts or its capture reserve fills.
pub(super) struct RenderState {
    processor: Processor,
    gain: f32,
    step: f32,
    started: bool,
    missing: bool,
    origin: [f32; 2],
    previous: [f32; 2],
}
impl RenderState {
    pub(super) fn new(settings: Settings, rate: u32) -> Self {
        Self {
            processor: Processor::new(settings),
            gain: 0.0,
            step: 1.0 / (rate.max(1) as f32 * 0.025),
            started: false,
            missing: false,
            origin: [0.0; 2],
            previous: [0.0; 2],
        }
    }
    /// Mixer inputs already have the clock bridge's startup ramp and a final
    /// output envelope. Preserve their existing startup response; retain gap recovery.
    pub(super) fn for_strip(settings: Settings, rate: u32) -> Self {
        Self {
            gain: 1.0,
            ..Self::new(settings, rate)
        }
    }
    pub(super) fn update(&mut self, settings: Settings) {
        self.processor.update(settings);
    }
    pub(super) fn settings_pending(&self) -> bool {
        self.processor.settings_pending()
    }
    fn resume_from_silence(&mut self) {
        self.gain = 0.0;
        self.origin = [0.0; 2];
        self.previous = [0.0; 2];
        self.missing = false;
        self.processor.reset_history();
    }
    pub(super) fn frame(
        &mut self,
        input: [f32; 2],
        source_ready: bool,
        stopping: bool,
    ) -> [f32; 2] {
        if !self.started && (!source_ready || stopping) {
            return [0.0; 2];
        }
        if !source_ready || stopping {
            if !self.missing {
                self.origin = self.previous;
                self.gain = 1.0;
                self.missing = true;
            }
            // Missing PCM is not valid silence or a filter input. Conceal only
            // this discontinuity with a bounded tail; do not smooth music peaks.
            self.gain = (self.gain - self.step).max(0.0);
            self.previous = self.origin.map(|sample| sample * self.gain);
            return self.previous;
        }
        if self.missing {
            self.origin = self.previous;
            self.gain = 0.0;
            self.missing = false;
            self.processor.reset_history();
        }
        self.started = true;
        self.gain = (self.gain + self.step).min(1.0);
        let processed = self.processor.process(input);
        self.previous = std::array::from_fn(|channel| {
            processed[channel] * self.gain + self.origin[channel] * (1.0 - self.gain)
        });
        self.previous
    }
}

pub fn input(
    device: &Device,
    config: &SupportedStreamConfig,
    queue: Arc<ArrayQueue<[f32; 2]>>,
    metrics: Arc<Metrics>,
) -> Result<Stream> {
    match config.sample_format() {
        SampleFormat::F32 => input_typed::<f32>(device, config, queue, metrics),
        SampleFormat::I16 => input_typed::<i16>(device, config, queue, metrics),
        SampleFormat::U16 => input_typed::<u16>(device, config, queue, metrics),
        SampleFormat::I32 => input_typed::<i32>(device, config, queue, metrics),
        _ => anyhow::bail!("Unsupported input sample format"),
    }
}
fn input_typed<T>(
    device: &Device,
    config: &SupportedStreamConfig,
    queue: Arc<ArrayQueue<[f32; 2]>>,
    metrics: Arc<Metrics>,
) -> Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels() as usize;
    let errors = metrics.clone();
    Ok(device.build_input_stream(
        &config.config(),
        move |data: &[T], _| {
            let frame_count = data.len() / channels;
            let mut dropped = 0_u64;
            for frame in data.chunks_exact(channels) {
                let left = f32::from_sample(frame[0]);
                let right = if channels == 1 {
                    left
                } else {
                    f32::from_sample(frame[1])
                };
                if queue.push([left, right]).is_err() {
                    dropped += 1;
                }
            }
            if dropped != 0 {
                metrics.overruns.fetch_add(dropped, Ordering::Relaxed);
            }
            metrics
                .captured_frames
                .fetch_add(frame_count as u64, Ordering::Relaxed);
        },
        move |_| {
            output_stream_error(&errors);
        },
        None,
    )?)
}

pub fn output(
    device: &Device,
    config: &SupportedStreamConfig,
    source: Source,
    settings: Settings,
    updates: Arc<ArrayQueue<Update>>,
    metrics: Arc<Metrics>,
) -> Result<Stream> {
    match config.sample_format() {
        SampleFormat::F32 => {
            output_typed::<f32>(device, config, source, settings, updates, metrics)
        }
        SampleFormat::I16 => {
            output_typed::<i16>(device, config, source, settings, updates, metrics)
        }
        SampleFormat::U16 => {
            output_typed::<u16>(device, config, source, settings, updates, metrics)
        }
        SampleFormat::I32 => {
            output_typed::<i32>(device, config, source, settings, updates, metrics)
        }
        _ => anyhow::bail!("Unsupported output sample format"),
    }
}
fn output_typed<T>(
    device: &Device,
    config: &SupportedStreamConfig,
    mut source: Source,
    settings: Settings,
    updates: Arc<ArrayQueue<Update>>,
    metrics: Arc<Metrics>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let mut renderer = Renderer::new(settings, config.sample_rate(), updates, metrics.clone());
    let errors = metrics.clone();
    Ok(device.build_output_stream(
        &config.config(),
        move |data: &mut [T], _| renderer.render(data, channels, || source.frame(&metrics)),
        move |_| {
            errors.errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )?)
}

pub(super) fn output_stream_error(metrics: &Metrics) {
    metrics.errors.fetch_add(1, Ordering::Relaxed);
    metrics
        .callback_discontinuities
        .fetch_add(1, Ordering::Release);
}

/// Shared PCM renderer for native callbacks and local-server workers.
/// Buffer transport, file access and route changes remain the caller's responsibility.
pub(super) struct Renderer {
    render: RenderState,
    updates: Arc<ArrayQueue<Update>>,
    metrics: Arc<Metrics>,
    worker: bool,
    initial: Option<Settings>,
    rate: u32,
    last_callback: Option<(Instant, usize)>,
    observed_discontinuities: u64,
}
impl Renderer {
    pub fn new(
        settings: Settings,
        rate: u32,
        updates: Arc<ArrayQueue<Update>>,
        metrics: Arc<Metrics>,
    ) -> Self {
        let observed_discontinuities = metrics.callback_discontinuities.load(Ordering::Acquire);
        Self {
            render: RenderState::new(settings, rate),
            updates,
            metrics,
            worker: false,
            initial: Some(settings),
            rate,
            last_callback: None,
            observed_discontinuities,
        }
    }
    #[cfg(unix)]
    pub fn for_worker(
        settings: Settings,
        rate: u32,
        updates: Arc<ArrayQueue<Update>>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            worker: true,
            ..Self::new(settings, rate, updates, metrics)
        }
    }
    pub fn render<T, F>(&mut self, data: &mut [T], channels: usize, source: F)
    where
        T: SizedSample + FromSample<f32>,
        F: FnMut() -> ([f32; 2], bool),
    {
        self.render_at(data, channels, source, Instant::now());
    }
    fn render_at<T, F>(&mut self, data: &mut [T], channels: usize, mut source: F, now: Instant)
    where
        T: SizedSample + FromSample<f32>,
        F: FnMut() -> ([f32; 2], bool),
    {
        let metrics = &self.metrics;
        if !(1..=2).contains(&channels) || !data.len().is_multiple_of(channels) {
            data.fill(T::from_sample(0.0));
            metrics.errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if data.is_empty() {
            return;
        }
        metrics.callback_ready.store(true, Ordering::Release);
        if metrics.output_quarantined.load(Ordering::Acquire) {
            data.fill(T::from_sample(0.0));
            return;
        }
        let discontinuities = metrics.callback_discontinuities.load(Ordering::Acquire);
        if discontinuities != self.observed_discontinuities {
            self.render.resume_from_silence();
            self.last_callback = None;
            self.observed_discontinuities = discontinuities;
        }
        let started = Instant::now();
        if let Some(settings) = self.initial.take() {
            metrics
                .tonal_bypass
                .store(settings.bypassed(), Ordering::Relaxed);
            metrics.effective_gain.store(
                (settings.effective_preamp_db() as f32).to_bits(),
                Ordering::Relaxed,
            );
        }
        let frame_count = data.len() / channels;
        if !self.worker {
            if let Some((previous, frames)) = self.last_callback {
                // Missing even one or two hardware deadlines can insert silence at the
                // device while capture continues to queue valid PCM. Treat that as a
                // discontinuity before replaying backlog. Allow half a callback period
                // of jitter plus 2 ms scheduler slack instead of a fixed 50 ms floor.
                let period = frames.max(1) as f64 / self.rate.max(1) as f64;
                let tolerance =
                    Duration::from_secs_f64(period * 1.5 + 0.002).max(Duration::from_millis(4));
                if now.saturating_duration_since(previous) > tolerance {
                    // Hardware has already had a gap: an old held sample cannot
                    // retroactively fade it. Restart from silence and flush live backlog.
                    self.render.resume_from_silence();
                    self.observed_discontinuities = metrics
                        .callback_discontinuities
                        .fetch_add(1, Ordering::Release)
                        .saturating_add(1);
                }
            }
            self.last_callback = Some((now, frame_count));
        }
        let mut latest = None;
        for _ in 0..8 {
            if let Some(update) = self.updates.pop() {
                latest = Some(update);
            } else {
                break;
            }
        }
        if let Some(update) = latest {
            self.render.update(update.settings);
            metrics
                .tonal_bypass
                .store(update.settings.bypassed(), Ordering::Relaxed);
            metrics.effective_gain.store(
                (update.settings.effective_preamp_db() as f32).to_bits(),
                Ordering::Relaxed,
            );
            metrics.revision.store(update.revision, Ordering::Relaxed);
            metrics
                .music_revision
                .store(update.music_revision, Ordering::Relaxed);
        }
        let stop = metrics.stopping.load(Ordering::Relaxed);
        let mut peak = [0.0_f32; 2];
        let mut analysis_dropped = 0_u64;
        for frame in data.chunks_exact_mut(channels) {
            let (input, ready) = source();
            if ready {
                if let Some(queue) = metrics.analysis_queue.get() {
                    if queue.push(input).is_err() {
                        analysis_dropped += 1;
                    }
                }
            }
            let mut processed = self.render.frame(input, ready, stop);
            if channels == 1 {
                processed = [(processed[0] + processed[1]) * 0.5; 2];
                frame[0] = T::from_sample(processed[0]);
            } else {
                frame[0] = T::from_sample(processed[0]);
                frame[1] = T::from_sample(processed[1]);
            }
            for c in 0..2 {
                peak[c] = peak[c].max(processed[c].abs());
            }
        }
        if analysis_dropped != 0 {
            metrics
                .analysis_dropped
                .fetch_add(analysis_dropped, Ordering::Relaxed);
        }
        metrics
            .source_started
            .store(self.render.started, Ordering::Relaxed);
        metrics
            .faded_out
            .store(stop && self.render.gain == 0.0, Ordering::Release);
        metrics.adaptive_reduction.store(
            (self.render.processor.adaptive_reduction_db() as f32).to_bits(),
            Ordering::Relaxed,
        );
        metrics.level_match_makeup.store(
            (self.render.processor.level_match_makeup_db() as f32).to_bits(),
            Ordering::Relaxed,
        );
        metrics.limiter_reduction.store(
            (self.render.processor.limiter_reduction_db() as f32).to_bits(),
            Ordering::Relaxed,
        );
        metrics
            .settings_transitioning
            .store(self.render.settings_pending(), Ordering::Relaxed);
        metrics
            .peak
            .store(peak[0].max(peak[1]).to_bits(), Ordering::Relaxed);
        metrics
            .peak_left
            .store(peak[0].to_bits(), Ordering::Relaxed);
        metrics
            .peak_right
            .store(peak[1].to_bits(), Ordering::Relaxed);
        metrics
            .frames
            .fetch_add(frame_count as u64, Ordering::Relaxed);
        let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        if self.worker {
            metrics
                .worker_processing_nanos
                .fetch_add(elapsed, Ordering::Relaxed);
            metrics
                .worker_frames
                .fetch_add(frame_count as u64, Ordering::Relaxed);
        } else {
            metrics
                .callback_processing_nanos
                .fetch_add(elapsed, Ordering::Relaxed);
            metrics
                .callback_max_nanos
                .fetch_max(elapsed, Ordering::Relaxed);
            metrics
                .callback_frames
                .fetch_add(frame_count as u64, Ordering::Relaxed);
            metrics.callback_calls.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A successful play request is not proof that the device started. Require an
/// actual callback before switching output or muting original application audio.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(super) fn wait_output_ready(metrics: &Metrics, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        anyhow::ensure!(
            metrics.errors.load(Ordering::Acquire) == 0,
            "Replacement output failed during startup"
        );
        if metrics.callback_ready.load(Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Replacement output did not start callbacks"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Caller prepares independent output telemetry and validates settings first.
/// The old stream remains owned until the replacement proves native readiness.
#[cfg(any(target_os = "windows", test))]
pub(super) fn handoff_output<T>(
    active: &mut Vec<T>,
    replacement: T,
    old: &Metrics,
    next: &Metrics,
    start: impl FnOnce(&T) -> Result<()>,
) -> Result<()> {
    anyhow::ensure!(
        next.output_quarantined.load(Ordering::Acquire),
        "Replacement output must start quarantined"
    );
    start(&replacement)?;
    wait_output_ready(next, Duration::from_secs(2))?;
    old.stopping.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_millis(120);
    while !old.faded_out.load(Ordering::Acquire) && Instant::now() < deadline {
        if next.errors.load(Ordering::Acquire) != 0 {
            old.stopping.store(false, Ordering::Release);
            anyhow::bail!("Replacement output failed while retiring the previous output");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if next.errors.load(Ordering::Acquire) != 0 {
        old.stopping.store(false, Ordering::Release);
        anyhow::bail!("Replacement output failed before handoff");
    }
    // Dropping the old stream joins its callback before the new one may dequeue.
    active.clear();
    active.push(replacement);
    next.output_quarantined.store(false, Ordering::Release);
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/limiter_telemetry.rs"]
mod limiter_telemetry_tests;
#[cfg(test)]
#[path = "../../tests/unit/loudness_recovery.rs"]
mod loudness_recovery_tests;
#[cfg(test)]
#[path = "../../tests/unit/loudness_transition.rs"]
mod loudness_transition_tests;
#[cfg(test)]
#[path = "../../tests/unit/output_transition.rs"]
mod output_transition_tests;
#[cfg(test)]
#[path = "../../tests/unit/output_render.rs"]
mod tests;
