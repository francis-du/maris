//! Fast visual telemetry is separate from the two-second evidence used by smart tuning.
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const BANDS: usize = 24;
const SIZE: usize = 4096;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spectrum {
    pub bands_dbfs: [f64; BANDS],
    pub updated_at_ms: u64,
    pub stage: &'static str,
}
pub struct Analyzer {
    fft: Arc<dyn Fft<f64>>,
    samples: Vec<[f32; 2]>,
    buffer: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    window: Vec<f64>,
    position: usize,
    frames: usize,
    elapsed: usize,
    rate: u32,
    scale: f64,
}
impl Analyzer {
    pub fn new(rate: u32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(SIZE);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        let window: Vec<f64> = (0..SIZE)
            .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (SIZE - 1) as f64).cos())
            .collect();
        let scale = 1.0 / (SIZE as f64 * window.iter().map(|x| x * x).sum::<f64>());
        Self {
            fft,
            scratch,
            window,
            scale,
            samples: vec![[0.0; 2]; SIZE],
            buffer: vec![Complex::default(); SIZE],
            position: 0,
            frames: 0,
            elapsed: 0,
            rate,
        }
    }
    pub fn push(&mut self, frame: [f32; 2]) -> Option<Spectrum> {
        self.samples[self.position] = frame.map(|v| if v.is_finite() { v } else { 0.0 });
        self.position = (self.position + 1) % SIZE;
        self.frames = (self.frames + 1).min(SIZE);
        self.elapsed += 1;
        if self.frames < SIZE || self.elapsed < (self.rate as usize / 30).max(1) {
            return None;
        }
        self.elapsed = 0;
        let mut power = [0.0_f64; BANDS];
        for channel in 0..2 {
            for i in 0..SIZE {
                self.buffer[i] = Complex::new(
                    self.samples[(self.position + i) % SIZE][channel] as f64 * self.window[i],
                    0.0,
                );
            }
            self.fft
                .process_with_scratch(&mut self.buffer, &mut self.scratch);
            for k in 1..SIZE / 2 {
                let hz = k as f64 * self.rate as f64 / SIZE as f64;
                if !(20.0..=20000.0).contains(&hz) {
                    continue;
                }
                let band = (((hz / 20.0).log10() / 3.0) * BANDS as f64).floor() as usize;
                power[band.min(BANDS - 1)] += self.buffer[k].norm_sqr() * self.scale;
            }
        }
        Some(Spectrum {
            bands_dbfs: power.map(|p| (10.0 * p.max(1e-12).log10()).clamp(-120.0, 24.0)),
            updated_at_ms: crate::analysis::now_ms(),
            stage: "pre_eq",
        })
    }
}

/// UI-only attack/release interpolation; stale or stopped telemetry decays to silence.
#[derive(Clone)]
pub struct Motion {
    pub bands: [f64; BANDS],
    pub levels: [f64; 2],
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            bands: [0.0; BANDS],
            levels: [0.0; 2],
        }
    }
}
impl Motion {
    pub fn update(&mut self, runtime: &serde_json::Value, dt: f64, now: u64) {
        let live = runtime["active"] == true
            && runtime["updated_at_ms"]
                .as_u64()
                .is_some_and(|t| now.saturating_sub(t) < 600);
        let fresh_spectrum = live
            && runtime["visualization"]["updated_at_ms"]
                .as_u64()
                .is_some_and(|t| now.saturating_sub(t) < 600);
        let smooth = |old: &mut f64, target: f64| {
            let tau = if target > *old { 0.04 } else { 0.25 };
            *old += (target - *old) * (1.0 - (-dt.clamp(0.0, 1.0) / tau).exp());
            if old.abs() < 0.0001 {
                *old = 0.0;
            }
        };
        for (i, bar) in self.bands.iter_mut().enumerate() {
            let db = if fresh_spectrum {
                runtime["visualization"]["bands_dbfs"][i]
                    .as_f64()
                    .unwrap_or(-120.0)
            } else {
                -120.0
            };
            smooth(bar, ((db + 72.0) / 72.0).clamp(0.0, 1.0));
        }
        for (i, key) in ["peak_left_dbfs", "peak_right_dbfs"].iter().enumerate() {
            let db = if live {
                runtime[*key].as_f64().unwrap_or(-120.0)
            } else {
                -120.0
            };
            smooth(&mut self.levels[i], ((db + 60.0) / 60.0).clamp(0.0, 1.0));
        }
    }
}
