//! CPU neural enhancement runs on a worker, never inside a CPAL callback.
use super::{bridge::LiveSource, Metrics};
use anyhow::Result;
use crossbeam_queue::ArrayQueue;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub struct VoiceWorker {
    pub output: Arc<ArrayQueue<[f32; 2]>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    vad: Arc<AtomicU32>,
    blocks: Arc<AtomicU64>,
    drops: Arc<AtomicU64>,
    recovery: Arc<Recovery>,
    thread: Option<JoinHandle<()>>,
}
impl VoiceWorker {
    #[cfg(not(feature = "neural"))]
    pub fn start(
        _input: Arc<ArrayQueue<[f32; 2]>>,
        _rate: u32,
        _metrics: Arc<Metrics>,
    ) -> Result<Self> {
        anyhow::bail!("RNNoise is not compiled in; rebuild with --features neural");
    }
    #[cfg(feature = "neural")]
    pub fn start(
        input: Arc<ArrayQueue<[f32; 2]>>,
        rate: u32,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        anyhow::ensure!(
            rate == 48000,
            "Neural voice processing requires 48 kHz endpoints"
        );
        let recovery = Arc::new(Recovery::new(metrics));
        let recovering = recovery.clone();
        let output = Arc::new(ArrayQueue::new(12000));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let vad = Arc::new(AtomicU32::new(0));
        let blocks = Arc::new(AtomicU64::new(0));
        let drops = Arc::new(AtomicU64::new(0));
        let (out, exit, running, probability, count, lost) = (
            output.clone(),
            stop.clone(),
            alive.clone(),
            vad.clone(),
            blocks.clone(),
            drops.clone(),
        );
        let thread = std::thread::Builder::new()
            .name("maris-rnnoise".into())
            .spawn(move || {
                struct Alive(Arc<AtomicBool>);
                impl Drop for Alive {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _alive = Alive(running);
                let mut model = crate::models::VoiceModel::new();
                let mut block = [[0.0_f32; 2]; 480];
                let mut processed = block;
                let mut observed = recovering
                    .observed
                    .each_ref()
                    .map(|slot| slot.load(Ordering::Acquire));
                let mut reset_initial = !recovering.configuration_valid();
                while !exit.load(Ordering::Relaxed) {
                    let current = recovering.snapshot();
                    if current != observed || reset_initial {
                        // Publish only after this sole producer has stopped writing.
                        // The input and output consumers each drain their own queue.
                        recovering.requested.fetch_add(1, Ordering::Release);
                        for _ in 0..input.capacity() {
                            if input.pop().is_none() {
                                break;
                            }
                        }
                        model = crate::models::VoiceModel::new();
                        probability.store(0.0_f32.to_bits(), Ordering::Relaxed);
                        for (slot, value) in recovering.observed.iter().zip(current) {
                            slot.store(value, Ordering::Relaxed);
                        }
                        observed = current;
                        reset_initial = false;
                    }
                    if recovering.metrics.errors.load(Ordering::Acquire) != 0 {
                        break;
                    }
                    let request = recovering.requested.load(Ordering::Acquire);
                    if !recovering.configuration_valid()
                        || recovering.drained.load(Ordering::Acquire) != request
                    {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    recovering.settled.store(request, Ordering::Release);
                    if !block_ready(&input, &out) {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    for frame in &mut block {
                        *frame = input.pop().unwrap_or([0.0; 2]);
                    }
                    let voice = model.process(&block, &mut processed);
                    probability.store(voice.to_bits(), Ordering::Relaxed);
                    count.fetch_add(1, Ordering::Relaxed);
                    // A capture notification can arrive during model inference.
                    // Do not publish that block; the next iteration resets history.
                    if !recovering.ready() {
                        continue;
                    }
                    for frame in processed {
                        if out.push(frame).is_err() {
                            lost.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })?;
        Ok(Self {
            output,
            stop,
            alive,
            vad,
            blocks,
            drops,
            recovery,
            thread: Some(thread),
        })
    }
    pub fn healthy(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub fn status(&self) -> Value {
        json!({"model":"rnnoise","local":true,"worker_alive":self.healthy(),"voice_probability":f32::from_bits(self.vad.load(Ordering::Relaxed)),"processed_blocks":self.blocks.load(Ordering::Relaxed),"dropped_frames":self.drops.load(Ordering::Relaxed),"recovery_generation":self.recovery.requested.load(Ordering::Acquire),"recovery_pending":!self.recovery.ready()})
    }
}
/// One producer/one consumer recovery handshake. It never acknowledges native
/// configuration on behalf of the control thread or drains the other consumer's queue.
pub(super) struct Recovery {
    metrics: Arc<Metrics>,
    requested: AtomicU64,
    drained: AtomicU64,
    settled: AtomicU64,
    observed: [AtomicU64; 4],
}
impl Recovery {
    #[cfg(feature = "neural")]
    fn new(metrics: Arc<Metrics>) -> Self {
        let result = Self {
            metrics,
            requested: AtomicU64::new(0),
            drained: AtomicU64::new(0),
            settled: AtomicU64::new(0),
            observed: std::array::from_fn(|_| AtomicU64::new(0)),
        };
        for (slot, value) in result.observed.iter().zip(result.snapshot()) {
            slot.store(value, Ordering::Relaxed);
        }
        result
    }
    fn snapshot(&self) -> [u64; 4] {
        [
            self.metrics.overruns.load(Ordering::Acquire),
            self.metrics.capture_discontinuities.load(Ordering::Acquire),
            self.metrics
                .callback_discontinuities
                .load(Ordering::Acquire),
            self.metrics.configuration_events.load(Ordering::Acquire),
        ]
    }
    fn configuration_valid(&self) -> bool {
        self.metrics.configuration_events.load(Ordering::Acquire)
            == self
                .metrics
                .checked_configuration_events
                .load(Ordering::Acquire)
    }
    pub(super) fn ready(&self) -> bool {
        let request = self.requested.load(Ordering::Acquire);
        request == self.settled.load(Ordering::Acquire)
            && self
                .observed
                .each_ref()
                .map(|slot| slot.load(Ordering::Acquire))
                == self.snapshot()
            && self.configuration_valid()
            && self.metrics.errors.load(Ordering::Acquire) == 0
            && request == self.requested.load(Ordering::Acquire)
    }
    pub(super) fn prepare_source(&self, bridge: &mut crate::dsp::resample::ClockBridge) -> bool {
        let request = self.requested.load(Ordering::Acquire);
        if self.ready() {
            return true;
        }
        // Drain once, only after the producer has published a paused request.
        // Repeating after acknowledgement could race the resumed producer and
        // discard its first fresh block. Each side owns just its own consumer.
        if request != self.settled.load(Ordering::Acquire)
            && request != self.drained.load(Ordering::Acquire)
        {
            bridge.reset();
            self.metrics.stream_resets.fetch_add(1, Ordering::Relaxed);
            if request == self.requested.load(Ordering::Acquire) {
                self.drained.store(request, Ordering::Release);
            }
        }
        false
    }
}

pub(super) fn prepare(
    input: Arc<ArrayQueue<[f32; 2]>>,
    input_rate: u32,
    output_rate: u32,
    enabled: bool,
    metrics: Arc<Metrics>,
) -> Result<(Option<VoiceWorker>, LiveSource)> {
    if enabled {
        let worker = VoiceWorker::start(input, input_rate, metrics)?;
        let source = LiveSource::with_rates(worker.output.clone(), input_rate, output_rate)?
            .with_voice_recovery(worker.recovery.clone());
        Ok((Some(worker), source))
    } else {
        Ok((
            None,
            LiveSource::with_rates(input, input_rate, output_rate)?,
        ))
    }
}

#[cfg(test)]
#[cfg(feature = "neural")]
#[path = "../../tests/unit/voice_recovery.rs"]
mod recovery_tests;

#[cfg(feature = "neural")]
fn block_ready(input: &ArrayQueue<[f32; 2]>, output: &ArrayQueue<[f32; 2]>) -> bool {
    // This worker is the only output producer. A consumer can only increase
    // available capacity, so reserve a whole block before consuming capture or
    // advancing model history. Waiting remains interruptible by the stop flag.
    input.len() >= 480 && output.capacity().saturating_sub(output.len()) >= 480
}

#[cfg(test)]
#[cfg(feature = "neural")]
#[path = "../../tests/unit/voice_backpressure.rs"]
mod tests;

impl Drop for VoiceWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
