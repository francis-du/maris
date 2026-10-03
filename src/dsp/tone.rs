//! Bounded music filters, using RBJ biquad equations (W3C Audio EQ Cookbook).
use anyhow::{ensure, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
    Notch,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub kind: Kind,
    pub frequency_hz: f64,
    pub gain_db: f64,
    pub q: f64,
}
impl Filter {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frequency_hz.is_finite() && (20.0..=20000.0).contains(&self.frequency_hz),
            "Filter frequency must be 20..20000 Hz"
        );
        ensure!(
            self.gain_db.is_finite() && (-12.0..=12.0).contains(&self.gain_db),
            "Filter gain must be -12..12 dB"
        );
        ensure!(
            self.q.is_finite() && (0.2..=10.0).contains(&self.q),
            "Filter Q must be 0.2..10"
        );
        Ok(())
    }
    pub fn compile(self, rate: u32) -> Result<Biquad> {
        self.validate()?;
        ensure!(
            (44100..=192000).contains(&rate) && self.frequency_hz < rate as f64 * 0.5,
            "Unsupported filter sample rate or Nyquist limit"
        );
        let w = 2.0 * PI * self.frequency_hz / rate as f64;
        let c = w.cos();
        let a = 10.0_f64.powf(self.gain_db / 40.0);
        let alpha = w.sin() / (2.0 * self.q);
        let beta = 2.0 * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match self.kind {
            Kind::Peak => (
                1.0 + alpha * a,
                -2.0 * c,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * c,
                1.0 - alpha / a,
            ),
            Kind::LowShelf => (
                a * ((a + 1.0) - (a - 1.0) * c + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - beta),
                (a + 1.0) + (a - 1.0) * c + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - beta,
            ),
            Kind::HighShelf => (
                a * ((a + 1.0) + (a - 1.0) * c + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - beta),
                (a + 1.0) - (a - 1.0) * c + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - beta,
            ),
            Kind::HighPass => (
                (1.0 + c) * 0.5,
                -(1.0 + c),
                (1.0 + c) * 0.5,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Kind::LowPass => (
                (1.0 - c) * 0.5,
                1.0 - c,
                (1.0 - c) * 0.5,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Kind::Notch => (1.0, -2.0 * c, 1.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
        };
        Ok(Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}
impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}
impl Biquad {
    pub fn allpass(frequency_hz: f64, q: f64, rate: u32) -> Result<Self> {
        ensure!(
            frequency_hz.is_finite()
                && q.is_finite()
                && (20.0..=20000.0).contains(&frequency_hz)
                && (0.2..=10.0).contains(&q)
                && (44100..=192000).contains(&rate)
                && frequency_hz < rate as f64 * 0.5,
            "Invalid all-pass parameters"
        );
        let w = 2.0 * PI * frequency_hz / rate as f64;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Ok(Self {
            b0: (1.0 - alpha) / a0,
            b1: (-2.0 * w.cos()) / a0,
            b2: 1.0,
            a1: (-2.0 * w.cos()) / a0,
            a2: (1.0 - alpha) / a0,
        })
    }

    pub fn bandpass(frequency_hz: f64, q: f64, rate: u32) -> Result<Self> {
        ensure!(
            frequency_hz.is_finite()
                && q.is_finite()
                && (20.0..=20000.0).contains(&frequency_hz)
                && (0.2..=10.0).contains(&q)
                && (44100..=192000).contains(&rate)
                && frequency_hz < rate as f64 * 0.5,
            "Invalid band-pass detector parameters"
        );
        let w = 2.0 * PI * frequency_hz / rate as f64;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Ok(Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha) / a0,
        })
    }

    pub fn response_db(&self, hz: f64, rate: u32) -> f64 {
        let w = 2.0 * PI * hz / rate as f64;
        let nr = self.b0 + self.b1 * w.cos() + self.b2 * (2.0 * w).cos();
        let ni = -self.b1 * w.sin() - self.b2 * (2.0 * w).sin();
        let dr = 1.0 + self.a1 * w.cos() + self.a2 * (2.0 * w).cos();
        let di = -self.a1 * w.sin() - self.a2 * (2.0 * w).sin();
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di))
            .max(1e-30)
            .log10()
    }
}
/// Control-thread maximum-response estimate; the final limiter still handles transients.
pub fn peak_response(rate: u32, response: impl Fn(f64) -> f64) -> f64 {
    let limit = rate as f64 * 0.49999;
    let mut peak = 0.0_f64;
    for i in 0..=4096 {
        let position = i as f64 / 4096.0;
        for hz in [limit.powf(position), (limit * position).max(0.1)] {
            let db = response(hz);
            if db.is_finite() {
                peak = peak.max(db);
            }
        }
    }
    peak
}

#[derive(Clone, Copy, Default)]
pub struct State {
    z1: f64,
    z2: f64,
}
impl State {
    pub fn process(&mut self, x: f64, c: Biquad) -> f64 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        if !y.is_finite() || !self.z1.is_finite() || !self.z2.is_finite() {
            *self = Self::default();
            return 0.0;
        }
        if self.z1.abs() < 1e-25 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-25 {
            self.z2 = 0.0;
        }
        y
    }
}
