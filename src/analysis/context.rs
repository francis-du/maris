//! Background music-context pipeline.
//! Semantic inference is pluggable; the integrated fallback publishes only measured signal context.
//! Raw audio never leaves the process and inference never runs on an audio callback.

use crate::analysis::Analysis;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub label: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignalContext {
    pub bass_energy: f64,
    pub treble_energy: f64,
    pub crest_db: f64,
    pub stereo_correlation: f64,
    pub rms_dbfs: f64,
    pub momentary_lufs: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MusicContext {
    pub genre: Vec<Tag>,
    pub instruments: Vec<Tag>,
    pub vocal_probability: Option<f64>,
    pub instrumental_probability: Option<f64>,
    pub mood: Vec<Tag>,
    pub confidence: f64,
    pub model: String,
    pub mode: String,
    pub updated_at_ms: u64,
    pub inference_ms: f64,
    pub signal: SignalContext,
}

impl MusicContext {
    pub fn signal_only(analysis: &Analysis) -> Self {
        let bass_energy = analysis.band_energy_share[..3].iter().sum::<f64>();
        let treble_energy = analysis.band_energy_share[7..].iter().sum::<f64>();
        Self {
            genre: Vec::new(),
            instruments: Vec::new(),
            vocal_probability: None,
            instrumental_probability: None,
            mood: Vec::new(),
            confidence: 0.0,
            model: "signal-only-v1".into(),
            mode: "signal_only".into(),
            updated_at_ms: analysis.updated_at_ms,
            inference_ms: 0.0,
            signal: SignalContext {
                bass_energy,
                treble_energy,
                crest_db: analysis.crest_db,
                stereo_correlation: analysis.stereo_correlation,
                rms_dbfs: analysis.rms_dbfs,
                momentary_lufs: analysis.momentary_lufs,
            },
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.genre.len() <= 16 && self.instruments.len() <= 32 && self.mood.len() <= 16,
            "Too many semantic tags"
        );
        for tag in self.genre.iter().chain(&self.instruments).chain(&self.mood) {
            ensure!(
                !tag.label.is_empty()
                    && tag.label.len() <= 128
                    && !tag.label.chars().any(char::is_control),
                "Invalid semantic label"
            );
            ensure!(
                tag.confidence.is_finite() && (0.0..=1.0).contains(&tag.confidence),
                "Invalid semantic confidence"
            );
        }
        for value in [self.vocal_probability, self.instrumental_probability] {
            ensure!(
                value.is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value)),
                "Invalid probability"
            );
        }
        ensure!(
            self.confidence.is_finite() && (0.0..=1.0).contains(&self.confidence),
            "Invalid overall confidence"
        );
        ensure!(
            !self.model.is_empty()
                && self.model.len() <= 128
                && !self.model.chars().any(char::is_control),
            "Invalid model identifier"
        );
        ensure!(
            matches!(self.mode.as_str(), "signal_only" | "semantic"),
            "Invalid music-context mode"
        );
        if self.mode == "signal_only" {
            ensure!(
                self.genre.is_empty()
                    && self.instruments.is_empty()
                    && self.mood.is_empty()
                    && self.vocal_probability.is_none()
                    && self.instrumental_probability.is_none()
                    && self.confidence == 0.0,
                "Signal-only context cannot contain semantic claims"
            );
        }
        ensure!(
            self.inference_ms.is_finite() && (0.0..=60_000.0).contains(&self.inference_ms),
            "Invalid inference latency"
        );
        ensure!(
            self.signal.bass_energy.is_finite()
                && self.signal.treble_energy.is_finite()
                && (0.0..=1.0).contains(&self.signal.bass_energy)
                && (0.0..=1.0).contains(&self.signal.treble_energy),
            "Invalid signal energy"
        );
        ensure!(
            self.signal.crest_db.is_finite()
                && self.signal.crest_db >= 0.0
                && self.signal.stereo_correlation.is_finite()
                && (-1.0..=1.0).contains(&self.signal.stereo_correlation)
                && self.signal.rms_dbfs.is_finite()
                && (-240.0..=30.0).contains(&self.signal.rms_dbfs)
                && self
                    .signal
                    .momentary_lufs
                    .is_none_or(|value| value.is_finite()),
            "Invalid signal context"
        );
        Ok(())
    }

    pub fn top_genre(&self) -> Option<&str> {
        self.genre
            .iter()
            .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
            .filter(|tag| tag.confidence >= 0.5)
            .map(|tag| tag.label.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct Window {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
    pub analysis: Analysis,
}

pub trait SemanticBackend: Send {
    fn id(&self) -> &'static str;
    fn infer(&mut self, input: &Window) -> Result<MusicContext>;
}

struct SignalOnlyBackend;
impl SemanticBackend for SignalOnlyBackend {
    fn id(&self) -> &'static str {
        "signal-only-v1"
    }

    fn infer(&mut self, input: &Window) -> Result<MusicContext> {
        let mut context = MusicContext::signal_only(&input.analysis);
        context.model = self.id().into();
        Ok(context)
    }
}

struct PendingWindow {
    generation: u64,
    window: Window,
}

#[derive(Default)]
struct ContextState {
    generation: u64,
    pending: Option<PendingWindow>,
    latest: Option<MusicContext>,
    last_accepted_ms: Option<u64>,
    last_error: Option<String>,
}

pub struct Worker {
    // This mailbox belongs to control/analysis workers only, never the audio callback.
    // One pending window is replaceable; reset clears it without waiting for inference.
    state: Arc<(Mutex<ContextState>, Condvar)>,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    skipped_interval: Arc<AtomicU64>,
    processed: Arc<AtomicU64>,
    failures: Arc<AtomicU64>,
    discarded: Arc<AtomicU64>,
    minimum_interval_ms: u64,
    backend_id: &'static str,
    semantic_backend_available: bool,
    initialization_error: Option<String>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start() -> Result<Self> {
        Self::start_from_model(crate::models::music_context_backend())
    }

    fn start_from_model(model: Result<Option<Box<dyn SemanticBackend>>>) -> Result<Self> {
        match model {
            Ok(Some(backend)) => Self::start_with_backend(backend),
            Ok(None) => Self::start_with_backend(Box::new(SignalOnlyBackend)),
            Err(error) => {
                // Recognition is optional. A failed model must not stop ordinary audio.
                let mut worker = Self::start_with_backend(Box::new(SignalOnlyBackend))?;
                worker.initialization_error =
                    Some(format!("{error:#}").chars().take(512).collect());
                Ok(worker)
            }
        }
    }

    pub fn start_with_backend(mut backend: Box<dyn SemanticBackend>) -> Result<Self> {
        let backend_id = backend.id();
        let semantic_backend_available = backend_id != "signal-only-v1";
        let state = Arc::new((Mutex::new(ContextState::default()), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let skipped_interval = Arc::new(AtomicU64::new(0));
        let processed = Arc::new(AtomicU64::new(0));
        let failures = Arc::new(AtomicU64::new(0));
        let discarded = Arc::new(AtomicU64::new(0));
        let minimum_interval_ms = if semantic_backend_available { 3_000 } else { 0 };
        let shared_state = state.clone();
        let shared_failures = failures.clone();
        let shared_discarded = discarded.clone();
        let shared_stop = stop.clone();
        let shared_processed = processed.clone();
        let thread = thread::Builder::new()
            .name("maris-music-context".into())
            .spawn(move || {
                while !shared_stop.load(Ordering::Acquire) {
                    let (lock, wake) = &*shared_state;
                    let Ok(state) = lock.lock() else {
                        break;
                    };
                    let Ok(mut state) = wake.wait_while(state, |state| {
                        state.pending.is_none() && !shared_stop.load(Ordering::Acquire)
                    }) else {
                        break;
                    };
                    if shared_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Some(job) = state.pending.take() else {
                        continue;
                    };
                    drop(state);
                    let began = Instant::now();
                    let outcome = backend.infer(&job.window).and_then(|mut context| {
                        context.model = backend_id.into();
                        // Observation time is controlled by the captured window, not by the model.
                        context.updated_at_ms = job.window.analysis.updated_at_ms;
                        // Recognition may add tags, never replace the measurements
                        // that the tuning proposal uses for level and frequency limits.
                        context.signal = MusicContext::signal_only(&job.window.analysis).signal;
                        context.inference_ms = began.elapsed().as_secs_f64() * 1000.0;
                        context.validate()?;
                        Ok(context)
                    });
                    let (context, error) = match outcome {
                        Ok(context) => (Some(context), None),
                        Err(error) => {
                            shared_failures.fetch_add(1, Ordering::Relaxed);
                            let fallback = MusicContext::signal_only(&job.window.analysis);
                            let context = fallback.validate().is_ok().then_some(fallback);
                            (
                                context,
                                Some(format!("{error:#}").chars().take(512).collect::<String>()),
                            )
                        }
                    };
                    if let Ok(mut state) = lock.lock() {
                        if state.generation == job.generation {
                            state.latest = context;
                            state.last_error = error;
                        } else {
                            shared_discarded.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    shared_processed.fetch_add(1, Ordering::Relaxed);
                }
            })?;
        Ok(Self {
            state,
            stop,
            dropped,
            skipped_interval,
            processed,
            failures,
            discarded,
            minimum_interval_ms,
            backend_id,
            semantic_backend_available,
            initialization_error: None,
            thread: Some(thread),
        })
    }

    pub fn submit(&self, window: Window) {
        if window.analysis.validate().is_err()
            || window.analysis.updated_at_ms > crate::analysis::now_ms()
            || window.rate != window.analysis.sample_rate
            || window.analysis.frames != window.frames.len() as u64
            || window.analysis.dropped_frames != 0
            || window.frames.is_empty()
            || window.frames.len() > window.rate as usize * 10
            || window
                .frames
                .iter()
                .flatten()
                .any(|sample| !sample.is_finite() || sample.abs() > 16.0)
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let updated_at_ms = window.analysis.updated_at_ms;
        let (lock, wake) = &*self.state;
        let Ok(mut state) = lock.lock() else {
            return;
        };
        // A delayed observation cannot replace a newer pending window or move
        // the rate limiter backwards. Reset clears this history for a new route.
        if state
            .last_accepted_ms
            .is_some_and(|last| updated_at_ms < last)
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if state
            .last_accepted_ms
            .is_some_and(|last| updated_at_ms - last < self.minimum_interval_ms)
        {
            self.skipped_interval.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if state.pending.is_some() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        state.pending = Some(PendingWindow {
            generation: state.generation,
            window,
        });
        state.last_accepted_ms = Some(updated_at_ms);
        wake.notify_one();
    }

    pub fn latest(&self) -> Option<MusicContext> {
        self.state
            .0
            .lock()
            .ok()
            .and_then(|state| state.latest.clone())
    }

    pub fn reset(&self) {
        if let Ok(mut state) = self.state.0.lock() {
            state.generation = state.generation.wrapping_add(1);
            state.latest = None;
            state.pending = None;
            state.last_accepted_ms = None;
            state.last_error = None;
        }
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "latest": self.latest(),
            "processed_windows": self.processed.load(Ordering::Relaxed),
            "dropped_windows": self.dropped.load(Ordering::Relaxed),
            "skipped_interval_windows": self.skipped_interval.load(Ordering::Relaxed),
            "minimum_interval_ms": self.minimum_interval_ms,
            "failed_windows": self.failures.load(Ordering::Relaxed),
            "discarded_generation_windows": self.discarded.load(Ordering::Relaxed),
            "last_error": self.state.0.lock().ok().and_then(|state| state.last_error.clone())
                .or_else(|| self.initialization_error.clone()),
            "initialization_error": self.initialization_error,
            "backend": self.backend_id,
            "semantic_backend_available": self.semantic_backend_available,
            "raw_audio_uploads": false
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(_guard) = self.state.0.lock() {
            self.state.1.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/context_start.rs"]
mod tests;

pub fn resample_mono_16k(frames: &[[f32; 2]], rate: u32) -> Result<Vec<f32>> {
    ensure!((8_000..=192_000).contains(&rate), "Unsupported source rate");
    if frames.is_empty() {
        return Ok(Vec::new());
    }
    let output_len = frames.len().saturating_mul(16_000) / rate as usize;
    // Retain this public helper's floor-rounded duration and empty-input behavior,
    // but use the same anti-alias filter as the actual model input.
    let mut output = crate::models::musicnn_preprocess::resample_mono(frames, rate)?;
    output.truncate(output_len);
    Ok(output)
}
