//! Bounded stereo sample-rate conversion and asynchronous device-clock correction.
//! Coefficients and buffers are allocated before stream startup; sample processing allocates nothing.
use crossbeam_queue::ArrayQueue;
use std::{f64::consts::PI, sync::Arc};

pub const TAPS: usize = 128;
const PHASES: usize = 256;
const CENTER: usize = TAPS / 2 - 1;

pub struct SincInterpolator {
    table: Box<[[f64; TAPS]]>,
    samples: Box<[[f32; 2]; TAPS]>,
    oldest: usize,
}
impl Default for SincInterpolator {
    fn default() -> Self {
        Self::new()
    }
}
impl SincInterpolator {
    pub fn new() -> Self {
        Self::with_cutoff(0.49)
    }
    fn with_cutoff(cutoff: f64) -> Self {
        let mut table = vec![[0.0; TAPS]; PHASES + 1].into_boxed_slice();
        for (phase, row) in table.iter_mut().enumerate() {
            let fraction = phase as f64 / PHASES as f64;
            for (i, coefficient) in row.iter_mut().enumerate() {
                let x = i as f64 - CENTER as f64 - fraction;
                let angle = 2.0 * PI * i as f64 / (TAPS - 1) as f64;
                let window = 0.42 - 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos();
                // Downsampling narrows the reconstruction passband to the destination Nyquist.
                let sinc = if x.abs() < 1e-12 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * x).sin() / (PI * x)
                };
                *coefficient = sinc * window;
            }
            let sum: f64 = row.iter().sum();
            for value in row {
                *value /= sum;
            }
        }
        Self {
            table,
            samples: Box::new([[0.0; 2]; TAPS]),
            oldest: 0,
        }
    }
    fn clear(&mut self) {
        self.samples.fill([0.0; 2]);
        self.oldest = 0;
    }
    pub fn push(&mut self, sample: [f32; 2]) {
        self.samples[self.oldest] = sample.map(|v| if v.is_finite() { v } else { 0.0 });
        self.oldest = (self.oldest + 1) % TAPS;
    }
    pub fn sample(&self, fraction: f64) -> [f32; 2] {
        let position = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0) * PHASES as f64
        } else {
            0.0
        };
        let phase = (position as usize).min(PHASES - 1);
        let weight = position - phase as f64;
        let a = &self.table[phase];
        let b = &self.table[phase + 1];
        let mut result = [0.0_f64; 2];
        for i in 0..TAPS {
            let coefficient = a[i] + (b[i] - a[i]) * weight;
            let value = self.samples[(self.oldest + i) % TAPS];
            result[0] += value[0] as f64 * coefficient;
            result[1] += value[1] as f64 * coefficient;
        }
        result.map(|v| v as f32)
    }
}

pub struct ClockBridge {
    queue: Arc<ArrayQueue<[f32; 2]>>,
    interpolator: SincInterpolator,
    target: usize,
    ready: bool,
    phase: f64,
    step: f64,
    nominal_step: f64,
    previous: [f32; 2],
    recovery: f32,
}
impl ClockBridge {
    pub fn new(queue: Arc<ArrayQueue<[f32; 2]>>, target: usize) -> Self {
        Self::with_step(queue, target, 1.0)
    }
    fn with_step(queue: Arc<ArrayQueue<[f32; 2]>>, target: usize, ratio: f64) -> Self {
        Self {
            queue,
            interpolator: SincInterpolator::with_cutoff(0.49 / ratio.max(1.0)),
            target: target.max(TAPS),
            ready: false,
            phase: 0.0,
            step: ratio,
            nominal_step: ratio,
            previous: [0.0; 2],
            recovery: 0.0,
        }
    }
    pub fn with_rates(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        target: usize,
        input_rate: u32,
        output_rate: u32,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (44100..=192000).contains(&input_rate) && (44100..=192000).contains(&output_rate),
            "Clock bridge rates must be 44100..192000 Hz"
        );
        let required = target
            .max(TAPS)
            .checked_add(TAPS + 8)
            .ok_or_else(|| anyhow::anyhow!("Clock bridge reserve is too large"))?;
        anyhow::ensure!(
            queue.capacity() >= required,
            "Clock bridge queue cannot hold its reserve and filter history"
        );
        let ratio = f64::from(input_rate) / f64::from(output_rate);
        // Construct the rate-specific table once, not a same-rate table followed
        // by a second allocation during each device rebuild.
        Ok(Self::with_step(queue, target, ratio))
    }
    /// Drop an interrupted stream's backlog with bounded work and no allocation.
    /// A concurrent producer cannot make the drain loop unbounded.
    pub fn reset(&mut self) {
        for _ in 0..self.queue.capacity() {
            if self.queue.pop().is_none() {
                break;
            }
        }
        self.interpolator.clear();
        self.ready = false;
        self.phase = 0.0;
        self.step = self.nominal_step;
        self.previous = [0.0; 2];
        self.recovery = 0.0;
    }
    /// True only after the capture reserve and interpolator history have been filled.
    /// A zero-valued source frame is still ready; this is not an amplitude detector.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Returns a frame and a newly observed underrun. Queue starvation is counted once per event.
    pub fn next_frame(&mut self) -> ([f32; 2], bool) {
        if !self.ready {
            if self.queue.len() < self.target.saturating_add(TAPS) {
                self.previous = self.previous.map(|v| v * 0.98);
                return (self.previous, false);
            }
            for _ in 0..TAPS {
                self.interpolator.push(self.queue.pop().unwrap_or([0.0; 2]));
            }
            self.phase = 0.0;
            self.step = self.nominal_step;
            self.recovery = 0.0;
            self.ready = true;
        }
        let error = (self.queue.len() as f64 - self.target as f64) / self.target as f64;
        let desired = self.nominal_step * (1.0 + (error * 0.0001).clamp(-0.001, 0.001));
        // Smooth callback-size bursts instead of modulating the sample clock at each burst.
        self.step += 0.0001 * (desired - self.step);
        let signal = self.interpolator.sample(self.phase);
        self.recovery = (self.recovery + 1.0 / 256.0).min(1.0);
        let output = signal.map(|v| v * self.recovery);
        self.phase += self.step;
        let mut underrun = false;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            if let Some(sample) = self.queue.pop() {
                self.interpolator.push(sample);
            } else {
                self.ready = false;
                underrun = true;
                break;
            }
        }
        self.previous = output;
        (output, underrun)
    }
}
