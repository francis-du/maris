//! Local pre-EQ signal statistics. This is signal analysis, not a trained scene classifier.
pub mod context;
pub mod spectrum;

use crate::{audio::offline, dsp::profile::FREQUENCIES};
use anyhow::{ensure, Result};
use crossbeam_queue::ArrayQueue;
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const BINS: usize = 10;
pub(crate) const MUSIC_WINDOW_SECONDS: usize = 3;
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    pub sample_rate: u32,
    pub frames: u64,
    pub rms_dbfs: f64,
    pub peak_dbfs: f64,
    #[serde(default)]
    pub momentary_lufs: Option<f64>,
    #[serde(default)]
    pub shortterm_lufs: Option<f64>,
    #[serde(default)]
    pub integrated_lufs: Option<f64>,
    #[serde(default)]
    pub true_peak_dbtp: Option<f64>,
    pub crest_db: f64,
    pub clipped_fraction: f64,
    pub band_energy_share: [f64; BINS],
    pub stereo_correlation: f64,
    pub dropped_frames: u64,
    pub updated_at_ms: u64,
    pub stage: String,
}
impl Analysis {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (44100..=192000).contains(&self.sample_rate),
            "Invalid analysis sample rate"
        );
        ensure!(
            self.frames > 0 && self.frames <= self.sample_rate as u64 * 300,
            "Invalid analysis frame count"
        );
        ensure!(
            self.rms_dbfs.is_finite() && (-240.0..=30.0).contains(&self.rms_dbfs),
            "Invalid RMS measurement"
        );
        ensure!(
            self.peak_dbfs.is_finite() && (-240.0..=30.0).contains(&self.peak_dbfs),
            "Invalid peak measurement"
        );
        for (name, value) in [
            ("momentary_lufs", self.momentary_lufs),
            ("shortterm_lufs", self.shortterm_lufs),
            ("integrated_lufs", self.integrated_lufs),
            ("true_peak_dbtp", self.true_peak_dbtp),
        ] {
            ensure!(
                value.is_none_or(|value| value.is_finite() && (-240.0..=30.0).contains(&value)),
                "Invalid {name} measurement"
            );
        }
        ensure!(
            self.crest_db.is_finite() && (0.0..=270.0).contains(&self.crest_db),
            "Invalid crest factor"
        );
        ensure!(
            self.clipped_fraction.is_finite() && (0.0..=1.0).contains(&self.clipped_fraction),
            "Invalid clipping measurement"
        );
        ensure!(
            self.stereo_correlation.is_finite() && (-1.0..=1.0).contains(&self.stereo_correlation),
            "Invalid channel correlation"
        );
        ensure!(
            self.band_energy_share
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "Invalid band energies"
        );
        let sum: f64 = self.band_energy_share.iter().sum();
        ensure!(
            sum == 0.0 || (sum - 1.0).abs() < 0.0001,
            "Band energies must sum to one or zero"
        );
        ensure!(self.stage == "pre_eq", "Expected pre-EQ analysis");
        Ok(())
    }
}
#[cfg(test)]
#[path = "../../tests/unit/test_clock.rs"]
pub(crate) mod test_clock;

pub fn now_ms() -> u64 {
    #[cfg(test)]
    if let Some(now) = test_clock::current() {
        return now;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn db(value: f64) -> f64 {
    20.0 * value.max(1e-12).log10()
}

fn loudness_meter(rate: u32) -> Result<ebur128::EbuR128> {
    Ok(ebur128::EbuR128::new(
        2,
        rate,
        ebur128::Mode::M | ebur128::Mode::S | ebur128::Mode::I | ebur128::Mode::TRUE_PEAK,
    )?)
}
fn feed_loudness(meter: &mut ebur128::EbuR128, frames: &[[f32; 2]]) -> Result<()> {
    let mut interleaved = Vec::with_capacity(frames.len() * 2);
    for frame in frames {
        interleaved.extend_from_slice(frame);
    }
    meter.add_frames_f32(&interleaved)?;
    Ok(())
}
fn finite_measurement(result: std::result::Result<f64, ebur128::Error>) -> Option<f64> {
    result.ok().filter(|value| value.is_finite())
}
fn apply_loudness(analysis: &mut Analysis, meter: &ebur128::EbuR128, previous_peak: bool) {
    analysis.momentary_lufs = finite_measurement(meter.loudness_momentary());
    analysis.shortterm_lufs = finite_measurement(meter.loudness_shortterm());
    analysis.integrated_lufs = finite_measurement(meter.loudness_global());
    let peak = if previous_peak {
        [meter.prev_true_peak(0), meter.prev_true_peak(1)]
    } else {
        [meter.true_peak(0), meter.true_peak(1)]
    };
    analysis.true_peak_dbtp = peak
        .into_iter()
        .filter_map(finite_measurement)
        .reduce(f64::max)
        .map(db);
}

fn measure_internal(frames: &[[f32; 2]], rate: u32, include_loudness: bool) -> Result<Analysis> {
    ensure!(
        (44100..=192000).contains(&rate),
        "Analysis supports 44.1..192 kHz"
    );
    ensure!(
        !frames.is_empty() && frames.len() <= rate as usize * 300,
        "Analyze 1 frame to 300 seconds at a time"
    );
    let fft_len = (rate as usize / 8).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);
    let mut spectrum = vec![Complex::new(0.0, 0.0); fft_len];
    let mut scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let window: Vec<f64> = (0..fft_len)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (fft_len - 1) as f64).cos())
        .collect();
    let mut energies = [0.0_f64; BINS];
    let mut powers = [0.0_f64; 2];
    let mut cross = 0.0;
    let mut peak = 0.0_f64;
    let mut clipped = 0_u64;
    for frame in frames {
        ensure!(
            frame.iter().all(|v| v.is_finite() && v.abs() <= 16.0),
            "Audio contains invalid or extreme samples"
        );
        for c in 0..2 {
            powers[c] += (frame[c] as f64).powi(2);
            peak = peak.max(frame[c].abs() as f64);
            if frame[c].abs() >= 0.999 {
                clipped += 1;
            }
        }
        cross += frame[0] as f64 * frame[1] as f64;
    }
    // Only complete windows contribute spectral evidence; never average L/R before FFT.
    for chunk in frames.chunks_exact(fft_len) {
        for channel in [0_usize, 1] {
            for (i, sample) in spectrum.iter_mut().enumerate() {
                *sample = Complex::new(chunk[i][channel] as f64 * window[i], 0.0);
            }
            fft.process_with_scratch(&mut spectrum, &mut scratch);
            for (k, value) in spectrum.iter().enumerate().take(fft_len / 2 + 1).skip(1) {
                let hz = k as f64 * rate as f64 / fft_len as f64;
                if !(20.0..=20000.0).contains(&hz) {
                    continue;
                }
                let band = FREQUENCIES
                    .windows(2)
                    .position(|pair| hz < (pair[0] * pair[1]).sqrt())
                    .unwrap_or(9);
                energies[band] += value.norm_sqr();
            }
        }
    }
    let total: f64 = energies.iter().sum();
    if total > 0.0 {
        energies.iter_mut().for_each(|v| *v /= total);
    }
    let rms = ((powers[0] + powers[1]) / (2 * frames.len()) as f64).sqrt();
    let denominator = (powers[0] * powers[1]).sqrt();
    let mut analysis = Analysis {
        sample_rate: rate,
        frames: frames.len() as u64,
        rms_dbfs: db(rms),
        peak_dbfs: db(peak),
        momentary_lufs: None,
        shortterm_lufs: None,
        integrated_lufs: None,
        true_peak_dbtp: None,
        crest_db: (db(peak) - db(rms)).max(0.0),
        clipped_fraction: clipped as f64 / (2 * frames.len()) as f64,
        band_energy_share: energies,
        stereo_correlation: if denominator > 1e-20 {
            (cross / denominator).clamp(-1.0, 1.0)
        } else {
            0.0
        },
        dropped_frames: 0,
        updated_at_ms: now_ms(),
        stage: "pre_eq".into(),
    };
    if include_loudness {
        if let Ok(mut meter) = loudness_meter(rate) {
            if feed_loudness(&mut meter, frames).is_ok() {
                apply_loudness(&mut analysis, &meter, false);
            }
        }
    }
    analysis.validate()?;
    Ok(analysis)
}
pub fn measure(frames: &[[f32; 2]], rate: u32) -> Result<Analysis> {
    measure_internal(frames, rate, true)
}
pub fn analyze_file(path: &std::path::Path) -> Result<Analysis> {
    let audio = offline::read_wav(path)?;
    let count = audio.frames.len().min(audio.rate as usize * 30);
    measure(&audio.frames[..count], audio.rate)
}

// Music recognition needs three complete seconds, independently of the two-second
// level measurements. Queued frames from before an observed loss are not joined
// to newer audio to make a seemingly continuous recognition window.
struct MusicWindowBuffer {
    frames: Vec<[f32; 2]>,
    required: usize,
    observed_dropped: u64,
    skip_queued: usize,
}
impl MusicWindowBuffer {
    fn new(rate: u32) -> Self {
        let required = rate as usize * MUSIC_WINDOW_SECONDS;
        Self {
            frames: Vec::with_capacity(required),
            required,
            observed_dropped: 0,
            skip_queued: 0,
        }
    }
    fn observe_loss(&mut self, dropped: u64, queued: usize) -> bool {
        if dropped == self.observed_dropped {
            return false;
        }
        self.observed_dropped = dropped;
        self.frames.clear();
        self.skip_queued = queued;
        true
    }
    fn push(&mut self, frame: [f32; 2]) -> bool {
        if self.skip_queued > 0 {
            self.skip_queued -= 1;
            return false;
        }
        self.frames.push(frame);
        self.frames.len() == self.required
    }
}

struct MusicObservation {
    dropped: u64,
    window: context::Window,
}

pub struct AnalysisWorker {
    pub queue: Arc<ArrayQueue<[f32; 2]>>,
    pub dropped: Arc<AtomicU64>,
    latest: Arc<Mutex<Option<Analysis>>>,
    visual: Arc<Mutex<Option<crate::analysis::spectrum::Spectrum>>>,
    context_window: Arc<Mutex<Option<MusicObservation>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl AnalysisWorker {
    pub fn start(rate: u32) -> Result<Self> {
        Self::start_with_dropped(rate, Arc::new(AtomicU64::new(0)))
    }
    /// Share the callback's loss counter so incomplete evidence cannot look reliable.
    pub fn start_with_dropped(rate: u32, dropped: Arc<AtomicU64>) -> Result<Self> {
        ensure!(
            (44100..=192000).contains(&rate),
            "Unsupported analysis sample rate"
        );
        let queue = Arc::new(ArrayQueue::new(rate as usize));
        let stop = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        let visual = Arc::new(Mutex::new(None));
        let context_window = Arc::new(Mutex::new(None));
        let shared_visual = visual.clone();
        let shared_context = context_window.clone();
        let (q, s, shared, lost) = (queue.clone(), stop.clone(), latest.clone(), dropped.clone());
        let thread = thread::Builder::new()
            .name("maris-analysis".into())
            .spawn(move || {
                let mut samples = Vec::with_capacity(rate as usize * 2);
                let mut music = MusicWindowBuffer::new(rate);
                let mut last_lost = 0;
                let mut visualizer = crate::analysis::spectrum::Analyzer::new(rate);
                let mut loudness = loudness_meter(rate).ok();
                while !s.load(Ordering::Relaxed) {
                    while samples.len() < rate as usize * 2 {
                        let dropped = lost.load(Ordering::Acquire);
                        if dropped != music.observed_dropped {
                            music.observe_loss(dropped, q.len());
                            if let Ok(mut window) = shared_context.lock() {
                                *window = None;
                            }
                        }
                        let Some(frame) = q.pop() else {
                            break;
                        };
                        if let Some(spectrum) = visualizer.push(frame) {
                            if let Ok(mut latest) = shared_visual.lock() {
                                *latest = Some(spectrum);
                            }
                        }
                        samples.push(frame);
                        if music.push(frame) {
                            // Measure this exact three-second observation, not the latest
                            // two-second meters. Inference stays on the separate model worker.
                            let observation = measure_internal(&music.frames, rate, true);
                            let lost_during_measurement =
                                music.observe_loss(lost.load(Ordering::Acquire), q.len());
                            if let Ok(mut window) = shared_context.lock() {
                                *window = if lost_during_measurement {
                                    None
                                } else {
                                    observation.ok().map(|analysis| MusicObservation {
                                        dropped: music.observed_dropped,
                                        window: context::Window {
                                            rate,
                                            frames: music.frames.clone(),
                                            analysis,
                                        },
                                    })
                                };
                            }
                            music.frames.clear();
                        }
                    }
                    if samples.len() == rate as usize * 2 {
                        let count = lost.load(Ordering::Relaxed);
                        if let Ok(mut result) = measure_internal(&samples, rate, false) {
                            if let Some(meter) = &mut loudness {
                                if feed_loudness(meter, &samples).is_ok() {
                                    apply_loudness(&mut result, meter, true);
                                }
                            }
                            result.dropped_frames = count.saturating_sub(last_lost);
                            if let Ok(mut lock) = shared.lock() {
                                *lock = Some(result.clone());
                            }
                        }
                        last_lost = count;
                        samples.clear();
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            })?;
        Ok(Self {
            queue,
            dropped,
            latest,
            visual,
            context_window,
            stop,
            thread: Some(thread),
        })
    }
    pub fn visual(&self) -> Option<crate::analysis::spectrum::Spectrum> {
        self.visual.lock().ok().and_then(|v| v.clone())
    }
    pub fn latest(&self) -> Option<Analysis> {
        self.latest.lock().ok().and_then(|v| v.clone())
    }
    /// Legacy audio-only access. Context inference must consume the paired observation.
    pub fn take_context_window(&self) -> Option<Vec<[f32; 2]>> {
        self.take_music_window().map(|window| window.frames)
    }
    pub fn take_music_window(&self) -> Option<crate::analysis::context::Window> {
        self.context_window
            .lock()
            .ok()
            .and_then(|mut value| value.take())
            // A loss can arrive while the analysis thread is busy measuring. Reject
            // its old mailbox immediately rather than waiting for its next queue poll.
            .filter(|observation| observation.dropped == self.dropped.load(Ordering::Acquire))
            .map(|observation| observation.window)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/analysis_windows.rs"]
mod window_tests;

impl Drop for AnalysisWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
