pub mod music;
pub mod profile;
pub mod resample;
pub mod tone;

use crate::dsp::profile::{Band, Profile};
use anyhow::{ensure, Result};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coefficients {
    pub fn peaking(band: Band, rate: u32) -> Self {
        let a = 10.0_f64.powf(band.gain_db / 40.0);
        let w = 2.0 * PI * band.frequency_hz / rate as f64;
        let alpha = w.sin() / (2.0 * band.q_at(rate));
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * w.cos() / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
    pub fn response_db(&self, frequency: f64, rate: u32) -> f64 {
        let w = 2.0 * PI * frequency / rate as f64;
        let nr = self.b0 + self.b1 * w.cos() + self.b2 * (2.0 * w).cos();
        let ni = -self.b1 * w.sin() - self.b2 * (2.0 * w).sin();
        let dr = 1.0 + self.a1 * w.cos() + self.a2 * (2.0 * w).cos();
        let di = -self.a1 * w.sin() - self.a2 * (2.0 * w).sin();
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di))
            .max(1e-30)
            .log10()
    }
}

pub const DSP_REVISION: &str = "music-fidelity-7";

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    coefficients: [Coefficients; 10],
    gain: f64,
    requested_preamp_db: f64,
    crossfeed: f64,
    width: f64,
    bypass: bool,
    lowpass: f64,
    release: f64,
    transition_frames: u32,
    music: crate::dsp::music::Settings,
}

impl Settings {
    pub fn with_music(
        mut self,
        profile: &crate::dsp::music::MusicProfile,
        rate: u32,
    ) -> Result<Self> {
        let mut music_profile = profile.clone();
        if self.bypass {
            // Global bypass must stay tonally dry, but the music stage still computes its
            // reference loudness so A/B does not become a volume comparison.
            music_profile.reference = true;
        }
        self.music = music_profile.compile(rate)?;
        // Bypass and processing share the same static safety gain so comparing them never
        // creates a loudness jump merely because headroom protection disappeared.
        let peak = crate::dsp::tone::peak_response(rate, |hz| {
            self.coefficients
                .iter()
                .map(|c| c.response_db(hz, rate))
                .sum::<f64>()
                + self.music.response_db(hz, rate)
        });
        let correction = if profile.enabled {
            profile.correction_preamp_db
        } else {
            0.0
        };
        let headroom = if peak > 0.01 { -peak - 0.5 } else { 0.0 };
        self.gain = 10.0_f64.powf(self.requested_preamp_db.min(correction).min(headroom) / 20.0);
        self.music.gain = 1.0; // Reserve headroom once, not separately in both processors.
        Ok(self)
    }
    /// Keep both sides of a startup transition at the same conservative safety gain.
    pub(crate) fn share_transition_headroom(mut self, mut target: Self) -> (Self, Self) {
        let gain = self.gain.min(target.gain);
        self.gain = gain;
        target.gain = gain;
        (self, target)
    }
    pub(crate) fn with_transition_ms(mut self, rate: u32, milliseconds: u32) -> Self {
        let frames =
            (u64::from(rate) * u64::from(milliseconds) / 1000).clamp(1, u64::from(u32::MAX)) as u32;
        self.transition_frames = frames;
        self
    }
    pub fn bypassed(&self) -> bool {
        self.bypass
    }
    pub fn effective_preamp_db(&self) -> f64 {
        20.0 * self.gain.max(1e-12).log10()
    }
    pub fn compile(profile: &Profile, rate: u32) -> Result<Self> {
        profile.validate()?;
        ensure!(
            (44100..=192000).contains(&rate),
            "Supported sample rates are 44100..192000 Hz"
        );
        ensure!(
            profile
                .bands
                .iter()
                .all(|b| b.frequency_hz < rate as f64 / 2.0),
            "EQ frequency must be below Nyquist"
        );
        Ok(Self {
            coefficients: profile.bands.map(|b| Coefficients::peaking(b, rate)),
            gain: 10.0_f64.powf(profile.effective_preamp_at(rate) / 20.0),
            requested_preamp_db: profile.preamp_db,
            crossfeed: profile.crossfeed,
            width: profile.stereo_width,
            bypass: profile.bypass,
            lowpass: 1.0 - (-2.0 * PI * 700.0 / rate as f64).exp(),
            release: (-1.0 / (rate as f64 * 0.08)).exp(),
            transition_frames: rate / 40,
            music: crate::dsp::music::MusicProfile {
                enabled: false,
                ..crate::dsp::music::MusicProfile::default()
            }
            .compile(rate)?,
        })
    }
}

#[derive(Clone, Copy, Default)]
struct FilterState {
    z1: f64,
    z2: f64,
}
impl FilterState {
    fn sample(&mut self, x: f64, c: Coefficients) -> f64 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        if self.z1.abs() < 1e-25 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-25 {
            self.z2 = 0.0;
        }
        y
    }
}

#[derive(Clone, Copy)]
struct Chain {
    settings: Settings,
    states: [[FilterState; 10]; 2],
    low: [f64; 2],
    music: crate::dsp::music::Processor,
}
impl Chain {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            states: [[FilterState::default(); 10]; 2],
            low: [0.0; 2],
            music: crate::dsp::music::Processor::new(settings.music),
        }
    }
    fn retune(&self, settings: Settings) -> Self {
        let mut next = Self::new(settings);
        next.low = self.low;
        next.music = self.music.retune(settings.music);
        for channel in 0..2 {
            for i in 0..10 {
                if settings.coefficients[i] == self.settings.coefficients[i] {
                    next.states[channel][i] = self.states[channel][i];
                }
            }
        }
        next
    }
    fn frame(&mut self, input: [f32; 2]) -> [f64; 2] {
        let s = self.settings;
        let mut v = input.map(|x| {
            if x.is_finite() {
                (x as f64).clamp(-16.0, 16.0) * s.gain
            } else {
                0.0
            }
        });
        if s.bypass {
            // The returned branch is the music processor's dry/reference signal. Its wet
            // branch runs only to derive attenuation-only loudness matching and keep state warm.
            return self.music.process(v);
        }
        for (channel, sample) in v.iter_mut().enumerate() {
            for (state, coeff) in self.states[channel].iter_mut().zip(s.coefficients) {
                *sample = state.sample(*sample, coeff);
            }
        }
        let mid = (v[0] + v[1]) * 0.5;
        let side = (v[0] - v[1]) * 0.5 * s.width;
        // Width changes the side component only. Do not divide the whole signal by width:
        // that incorrectly attenuates mono/center content (for example -3.52 dB at 150%).
        v = [mid + side, mid - side];
        for (low, sample) in self.low.iter_mut().zip(v) {
            *low += s.lowpass * (sample - *low);
        }
        self.music.process([
            (v[0] + s.crossfeed * self.low[1]) / (1.0 + s.crossfeed),
            (v[1] + s.crossfeed * self.low[0]) / (1.0 + s.crossfeed),
        ])
    }
    fn adaptive_reduction_db(&self) -> f64 {
        self.music.adaptive_reduction_db()
    }
}

pub struct Processor {
    active: Chain,
    next: Chain,
    remaining: u32,
    total: u32,
    limiter_gain: f64,
    pending: Option<Settings>,
}
impl Processor {
    pub fn new(settings: Settings) -> Self {
        Self {
            active: Chain::new(settings),
            next: Chain::new(settings),
            remaining: 0,
            total: 1,
            limiter_gain: 1.0,
            pending: None,
        }
    }
    pub fn update(&mut self, settings: Settings) {
        // Entire chains are crossfaded; interpolating raw IIR coefficients is avoided.
        if self.remaining > 0 {
            self.pending = Some(settings);
            return;
        }
        if settings == self.active.settings {
            return;
        }
        self.next = self.active.retune(settings);
        self.total = settings.transition_frames.max(1);
        self.remaining = self.total;
    }
    /// A discontinuity starts new filter history without reallocating, changing
    /// settings, forgetting queued transitions, or releasing limiter attenuation.
    pub(crate) fn reset_history(&mut self) {
        self.active = Chain::new(self.active.settings);
        self.next = Chain::new(self.next.settings);
    }
    pub fn adaptive_reduction_db(&self) -> f64 {
        if self.remaining > 0 {
            self.active
                .adaptive_reduction_db()
                .max(self.next.adaptive_reduction_db())
        } else {
            self.active.adaptive_reduction_db()
        }
    }
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut y = self.active.frame(input);
        if self.remaining > 0 {
            let other = self.next.frame(input);
            let t = 1.0 - self.remaining as f64 / self.total as f64;
            for c in 0..2 {
                y[c] = y[c] * (1.0 - t) + other[c] * t;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.active = self.next;
                if let Some(settings) = self.pending.take() {
                    self.update(settings);
                }
            }
        }
        if y.iter().any(|v| !v.is_finite()) {
            y = [0.0; 2];
        }
        let peak = y[0].abs().max(y[1].abs());
        const CEILING: f64 = 0.8912509381337456; // -1 dBFS sample peak, not true peak or SPL.
        let wanted = if peak > CEILING { CEILING / peak } else { 1.0 };
        if wanted < self.limiter_gain {
            self.limiter_gain = wanted;
        } else {
            self.limiter_gain = self.active.settings.release * self.limiter_gain
                + (1.0 - self.active.settings.release) * wanted;
        }
        y.map(|v| (v * self.limiter_gain).clamp(-CEILING, CEILING) as f32)
    }
}
