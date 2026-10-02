//! MusicNN log-mel preparation with anti-aliased, worker-only resampling.
//! The bounded sinc filter is not a claim of numerical parity with a reference library.
use anyhow::{ensure, Result};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::{Arc, OnceLock};

const RATE: u32 = 16_000;
const FFT: usize = 512;
const HOP: usize = 256;
const MELS: usize = 96;
const PATCH_FRAMES: usize = 187;

pub(crate) fn resample_mono(frames: &[[f32; 2]], source_rate: u32) -> Result<Vec<f32>> {
    ensure!(
        (8_000..=384_000).contains(&source_rate),
        "Invalid MusicNN source sample rate"
    );
    ensure!(!frames.is_empty(), "MusicNN input is empty");
    let mono: Vec<f64> = frames
        .iter()
        .map(|frame| {
            let l = if frame[0].is_finite() { frame[0] } else { 0.0 };
            let r = if frame[1].is_finite() { frame[1] } else { 0.0 };
            (l as f64 + r as f64) * 0.5
        })
        .collect();
    if source_rate == RATE {
        return Ok(mono.into_iter().map(|v| v as f32).collect());
    }
    let output_len = ((mono.len() as u64 * RATE as u64 + source_rate as u64 / 2)
        / source_rate as u64)
        .max(1) as usize;
    // Narrow the passband before downsampling. Scale the filter support with
    // the input/output ratio so high-rate devices do not lose stopband rejection.
    let ratio = source_rate as f64 / RATE as f64;
    let cutoff = 1.0 / ratio.max(1.0);
    let radius = (16.0 * ratio.max(1.0)).ceil() as i64;
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let position = index as f64 * ratio;
        let center = position.floor() as i64;
        let mut sum = 0.0;
        let mut weight_sum = 0.0;
        for tap in -radius..=radius {
            let source = center + tap;
            if source < 0 || source >= mono.len() as i64 {
                continue;
            }
            let x = position - source as f64;
            let sinc = if x.abs() < 1e-12 {
                cutoff
            } else {
                let px = std::f64::consts::PI * x;
                (px * cutoff).sin() / px
            };
            let normalized = x / (radius as f64 + 1.0);
            if normalized.abs() >= 1.0 {
                continue;
            }
            let window = 0.5 + 0.5 * (std::f64::consts::PI * normalized).cos();
            let weight = sinc * window;
            sum += mono[source as usize] * weight;
            weight_sum += weight;
        }
        let value = if weight_sum.abs() > 1e-12 {
            sum / weight_sum
        } else {
            0.0
        };
        output.push(value.clamp(-16.0, 16.0) as f32);
    }
    Ok(output)
}

fn hz_to_mel(freq: f64) -> f64 {
    const F_MIN: f64 = 0.0;
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    const MIN_LOG_MEL: f64 = (MIN_LOG_HZ - F_MIN) / F_SP;
    let log_step = 6.4_f64.ln() / 27.0;
    if freq >= MIN_LOG_HZ {
        MIN_LOG_MEL + (freq / MIN_LOG_HZ).ln() / log_step
    } else {
        (freq - F_MIN) / F_SP
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    const F_MIN: f64 = 0.0;
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    const MIN_LOG_MEL: f64 = (MIN_LOG_HZ - F_MIN) / F_SP;
    let log_step = 6.4_f64.ln() / 27.0;
    if mel >= MIN_LOG_MEL {
        MIN_LOG_HZ * (log_step * (mel - MIN_LOG_MEL)).exp()
    } else {
        F_MIN + F_SP * mel
    }
}

fn mel_filters() -> Vec<[f32; FFT / 2 + 1]> {
    let min = hz_to_mel(0.0);
    let max = hz_to_mel(RATE as f64 * 0.5);
    let points: Vec<f64> = (0..MELS + 2)
        .map(|i| mel_to_hz(min + (max - min) * i as f64 / (MELS + 1) as f64))
        .collect();
    let fft_freq: Vec<f64> = (0..=FFT / 2)
        .map(|bin| bin as f64 * RATE as f64 / FFT as f64)
        .collect();
    (0..MELS)
        .map(|mel| {
            let lower = points[mel];
            let center = points[mel + 1];
            let upper = points[mel + 2];
            let norm = 2.0 / (upper - lower);
            let mut weights = [0.0_f32; FFT / 2 + 1];
            for (bin, freq) in fft_freq.iter().copied().enumerate() {
                let down = (freq - lower) / (center - lower);
                let up = (upper - freq) / (upper - center);
                weights[bin] = down.min(up).max(0.0).mul_add(norm, 0.0) as f32;
            }
            weights
        })
        .collect()
}

pub(super) fn mel_patch(samples: &[f32]) -> Result<Vec<f32>> {
    ensure!(
        samples.len() >= RATE as usize * crate::analysis::MUSIC_WINDOW_SECONDS,
        "Music recognition needs three complete seconds of audio"
    );
    // The analysis worker always uses the same transform and filter bank.
    // Share immutable setup; keep scratch local so simultaneous workers cannot interfere.
    static FILTERS: OnceLock<Vec<[f32; FFT / 2 + 1]>> = OnceLock::new();
    static TRANSFORM: OnceLock<Arc<dyn Fft<f32>>> = OnceLock::new();
    static HANN: OnceLock<[f32; FFT]> = OnceLock::new();
    let filters = FILTERS.get_or_init(mel_filters);
    let fft = TRANSFORM.get_or_init(|| FftPlanner::<f32>::new().plan_fft_forward(FFT));
    let hann = HANN.get_or_init(|| {
        std::array::from_fn(|n| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / FFT as f32).cos()
        })
    });
    let mut scratch = vec![Complex::new(0.0_f32, 0.0); fft.get_inplace_scratch_len()];
    // Centering pads the transform's left edge only; the required audio above
    // contains every sample used by all 187 frames. Never fabricate missing time.
    let mut patch = vec![0.0_f32; PATCH_FRAMES * MELS];
    let mut buffer = vec![Complex::new(0.0_f32, 0.0); FFT];
    for frame in 0..PATCH_FRAMES {
        let start = frame as isize * HOP as isize - (FFT / 2) as isize;
        for i in 0..FFT {
            let source = start + i as isize;
            let sample = if source >= 0 && (source as usize) < samples.len() {
                samples[source as usize]
            } else {
                0.0
            };
            buffer[i] = Complex::new(sample * hann[i], 0.0);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        let mut power = [0.0_f32; FFT / 2 + 1];
        for bin in 0..=FFT / 2 {
            power[bin] = buffer[bin].norm_sqr();
        }
        for mel in 0..MELS {
            let energy = power
                .iter()
                .zip(filters[mel].iter())
                .map(|(value, weight)| value * weight)
                .sum::<f32>();
            patch[frame * MELS + mel] = (10_000.0_f32.mul_add(energy, 1.0)).log10();
        }
    }
    ensure!(
        patch.iter().all(|value| value.is_finite()),
        "MusicNN preprocessing produced non-finite values"
    );
    Ok(patch)
}

#[cfg(test)]
#[path = "../../tests/unit/musicnn_preprocess.rs"]
mod tests;
