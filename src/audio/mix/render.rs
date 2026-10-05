//! One master output consumes every source once; other outputs receive separate bus queues.
use super::super::{
    bridge::{LiveSource, RenderState},
    Metrics,
};
use crate::{
    dsp,
    dsp::music::MusicProfile,
    dsp::profile::Profile,
    mixer::{self, MixerConfig, BUS_COUNT, MAX_STRIPS},
};
use anyhow::Result;
use crossbeam_queue::ArrayQueue;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
pub(super) struct Update {
    matrix: mixer::engine::Settings,
    strips: [dsp::Settings; MAX_STRIPS],
    revision: u64,
}
impl Update {
    pub fn compile(config: &MixerConfig, revision: u64, rate: u32) -> Result<Self> {
        config.validate()?;
        let flat = Profile::default();
        let neutral = dsp::Settings::compile(&flat, rate)?;
        let mut strips = [neutral; MAX_STRIPS];
        for (index, strip) in config.strips.iter().enumerate() {
            let music = MusicProfile {
                compressor: strip.compressor,
                adaptive: crate::dsp::music::AdaptiveEq {
                    enabled: false,
                    strength: 0.0,
                },
                level_match: false,
                ..MusicProfile::default()
            };
            strips[index] = dsp::Settings::compile(strip.eq.as_ref().unwrap_or(&flat), rate)?
                .with_music(&music, rate)?;
        }
        Ok(Self {
            matrix: mixer::engine::Settings::compile(config, rate)?,
            strips,
            revision,
        })
    }
}

pub(super) struct Input {
    source: LiveSource,
    pub metrics: Arc<Metrics>,
    pub peak: Arc<AtomicU64>,
}
impl Input {
    #[cfg(test)]
    pub fn new(queue: Arc<ArrayQueue<[f32; 2]>>, rate: u32, metrics: Arc<Metrics>) -> Self {
        Self::with_rates(queue, rate, rate, metrics).expect("valid test input rates")
    }
    pub fn with_rates(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        input_rate: u32,
        mix_rate: u32,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        Ok(Self {
            source: LiveSource::with_rates(queue, input_rate, mix_rate)?,
            metrics,
            peak: Arc::new(AtomicU64::new(0)),
        })
    }
}
struct BusDelay {
    buffer: Box<[[f64; 2]]>,
    cursor: usize,
    delay: usize,
}
impl BusDelay {
    fn new(rate: u32, delay_ms: f64) -> Result<Self> {
        anyhow::ensure!(
            delay_ms.is_finite() && (0.0..=500.0).contains(&delay_ms),
            "Bus delay must be 0..500 ms"
        );
        let max_frames = (usize::try_from(rate)? / 2).saturating_add(2);
        let delay = ((f64::from(rate) * delay_ms / 1000.0).round() as usize).min(max_frames - 1);
        Ok(Self {
            buffer: vec![[0.0; 2]; max_frames].into_boxed_slice(),
            cursor: 0,
            delay,
        })
    }
    fn process(&mut self, input: [f64; 2]) -> [f64; 2] {
        if self.delay == 0 {
            return input;
        }
        let len = self.buffer.len();
        let read = (self.cursor + len - self.delay) % len;
        let output = self.buffer[read];
        self.buffer[self.cursor] = input;
        self.cursor = (self.cursor + 1) % len;
        output
    }
}

pub(super) struct GraphConfig {
    pub inputs: Vec<Option<Input>>,
    pub initial: Update,
    pub delay_ms: [f64; BUS_COUNT],
    pub rate: u32,
    pub primary: usize,
    pub sends: [Option<Arc<ArrayQueue<[f32; 2]>>>; BUS_COUNT],
    pub send_metrics: [Arc<Metrics>; BUS_COUNT],
    pub updates: Arc<ArrayQueue<Update>>,
    pub revision: Arc<AtomicU64>,
}

pub(super) struct Graph {
    inputs: Vec<Option<Input>>,
    processors: Vec<RenderState>,
    settings: [dsp::Settings; MAX_STRIPS],
    matrix: mixer::Processor,
    delays: [BusDelay; BUS_COUNT],
    primary: usize,
    sends: [Option<Arc<ArrayQueue<[f32; 2]>>>; BUS_COUNT],
    send_metrics: [Arc<Metrics>; BUS_COUNT],
    updates: Arc<ArrayQueue<Update>>,
    revision: Arc<AtomicU64>,
    pending_revision: Option<u64>,
    pending_send_overruns: [u64; BUS_COUNT],
    observed_callback_epoch: u64,
}
impl Graph {
    pub fn new(config: GraphConfig) -> Result<Self> {
        let GraphConfig {
            inputs,
            initial,
            delay_ms,
            rate,
            primary,
            sends,
            send_metrics,
            updates,
            revision,
        } = config;
        let processors = initial.strips[..inputs.len()]
            .iter()
            .map(|s| RenderState::for_strip(*s, rate))
            .collect();
        Ok(Self {
            inputs,
            processors,
            settings: initial.strips,
            matrix: mixer::Processor::compiled(initial.matrix),
            delays: [
                BusDelay::new(rate, delay_ms[0])?,
                BusDelay::new(rate, delay_ms[1])?,
            ],
            primary,
            sends,
            send_metrics,
            updates,
            revision,
            pending_revision: None,
            pending_send_overruns: [0; BUS_COUNT],
            observed_callback_epoch: 0,
        })
    }
    pub fn begin_block(&mut self) {
        let mut latest = None;
        for _ in 0..2 {
            if let Some(update) = self.updates.pop() {
                latest = Some(update);
            } else {
                break;
            }
        }
        if let Some(update) = latest {
            self.matrix.update(update.matrix);
            for (index, processor) in self.processors.iter_mut().enumerate() {
                if self.settings[index] != update.strips[index] {
                    processor.update(update.strips[index]);
                }
            }
            self.settings = update.strips;
            self.pending_revision = Some(update.revision);
            self.commit_revision_if_settled();
        }
    }
    fn processing_pending(&self) -> bool {
        self.matrix.settings_pending()
            || self
                .processors
                .iter()
                .zip(&self.inputs)
                .any(|(processor, input)| input.is_some() && processor.settings_pending())
    }
    fn commit_revision_if_settled(&mut self) {
        if self.processing_pending() {
            return;
        }
        if let Some(revision) = self.pending_revision.take() {
            self.revision.store(revision, Ordering::Release);
        }
    }
    pub fn end_block(&mut self) {
        for bus in 0..BUS_COUNT {
            let dropped = std::mem::take(&mut self.pending_send_overruns[bus]);
            if dropped != 0 {
                self.send_metrics[bus]
                    .overruns
                    .fetch_add(dropped, Ordering::Relaxed);
            }
        }
    }
    pub fn frame(&mut self) -> ([f32; 2], bool) {
        let callback_epoch = self.send_metrics[self.primary]
            .callback_discontinuities
            .load(Ordering::Acquire);
        if callback_epoch != self.observed_callback_epoch {
            for delay in &mut self.delays {
                delay.buffer.fill([0.0; 2]);
                delay.cursor = 0;
            }
            for input in self.inputs.iter().flatten() {
                input
                    .metrics
                    .callback_discontinuities
                    .fetch_add(1, Ordering::Release);
            }
            self.observed_callback_epoch = callback_epoch;
        }
        let mut frames = [[0.0; 2]; MAX_STRIPS];
        let mut ready = false;
        for (index, input) in self.inputs.iter_mut().enumerate() {
            if let Some(input) = input {
                let (frame, source_ready) = input.source.frame(&input.metrics);
                ready |= source_ready;
                frames[index] = self.processors[index].frame(frame, source_ready, false);
                let peak = frames[index][0].abs().max(frames[index][1].abs()) as f64;
                input.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
            }
        }
        let mut mixed = self.matrix.process_unlimited(&frames[..self.inputs.len()]);
        for (bus, frame) in mixed.iter_mut().enumerate() {
            *frame = self.delays[bus].process(*frame);
        }
        for (bus, frame) in mixed.iter().enumerate() {
            if ready {
                if let Some(queue) = &self.sends[bus] {
                    if queue.push(frame.map(|v| v as f32)).is_err() {
                        self.pending_send_overruns[bus] += 1;
                    }
                }
            }
        }
        self.commit_revision_if_settled();
        (mixed[self.primary].map(|v| v as f32), ready)
    }
}
