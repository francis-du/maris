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

pub const DSP_REVISION: &str = "music-fidelity-10";

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    coefficients: [Coefficients; 10],
    gain: f64,
    level_target_gain: f64,
    requested_preamp_db: f64,
    level_match: bool,
    match_meter: f64,
    match_slew: f64,
    match_transition_slew: f64,
    match_pre_filter: crate::dsp::tone::Biquad,
    match_rlb_filter: crate::dsp::tone::Biquad,
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
        // Safety headroom belongs before EQ/dynamics, but it must not become a permanent
        // loudness cut versus the original system path. Keep the user's requested preamp as
        // the post-processing level-match target; the final limiter remains authoritative for
        // instantaneous peak safety.
        self.level_target_gain = 10.0_f64.powf(self.requested_preamp_db / 20.0);
        self.level_match = profile.level_match;
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
        let gain = 10.0_f64.powf(profile.effective_preamp_at(rate) / 20.0);
        Ok(Self {
            coefficients: profile.bands.map(|b| Coefficients::peaking(b, rate)),
            gain,
            level_target_gain: gain,
            requested_preamp_db: profile.preamp_db,
            level_match: false,
            match_meter: (-1.0 / (rate as f64 * 1.5)).exp(),
            match_slew: (-1.0 / (rate as f64 * 0.25)).exp(),
            // Retunes already have an audible crossfade, but detector-driven target gain can
            // still move sharply inside that envelope. A 0.25 ms dezipper prevents per-frame
            // gain jumps while remaining negligible beside a 120 ms settings transition.
            match_transition_slew: (-1.0 / (rate as f64 * 0.00025)).exp(),
            // Approximate the BS.1770 K-weighting detector with the standard pre-filter
            // and RLB corner parameters, compiled through the existing allocation-free
            // RBJ biquads. This keeps the audio callback real-time safe while making Level
            // Match track perceived program loudness instead of unweighted RMS alone.
            match_pre_filter: crate::dsp::tone::Filter {
                kind: crate::dsp::tone::Kind::HighShelf,
                frequency_hz: 1_681.974_450_955_533,
                gain_db: 3.999_843_853_973_347,
                q: 0.707_175_236_955_419_6,
            }
            .compile(rate)?,
            match_rlb_filter: crate::dsp::tone::Filter {
                kind: crate::dsp::tone::Kind::HighPass,
                frequency_hz: 38.135_470_876_024_44,
                gain_db: 0.0,
                q: 0.500_327_037_323_877_3,
            }
            .compile(rate)?,
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
        next.music = self.music.retune(settings.music);
        // Global bypass skips the outer EQ and crossfeed-lowpass path entirely.
        // Do not carry state across a bypass boundary or that frozen history can
        // reappear when processing is re-enabled after an arbitrary dry interval.
        if !self.settings.bypass && !settings.bypass {
            // An IIR stage's state is only reusable when its complete upstream feed is
            // unchanged. Once one earlier stage (or the pre-EQ gain) changes, every
            // downstream stage must start fresh even if its own coefficients match.
            let same_gain = self.settings.gain == settings.gain;
            let mut prefix_unchanged = same_gain;
            for i in 0..10 {
                prefix_unchanged &= settings.coefficients[i] == self.settings.coefficients[i];
                if prefix_unchanged {
                    for channel in 0..2 {
                        next.states[channel][i] = self.states[channel][i];
                    }
                }
            }
            // Crossfeed's lowpass is fed after the full EQ cascade and stereo-width
            // transform, so its history is valid only when that entire feed is stable.
            if prefix_unchanged && self.settings.width == settings.width {
                next.low = self.low;
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
        let static_makeup = static_level_match_gain(&s);
        if s.bypass {
            // The returned branch is the music processor's dry/reference signal. Its wet
            // branch runs only to derive loudness matching and keep state warm. Known static
            // headroom is restored inside each chain so settings crossfades stay level-safe.
            return self.music.process(v).map(|sample| sample * static_makeup);
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
        // Crossfeed should redistribute only the low-frequency side component.
        // The previous convex mix divided the entire direct signal by (1 + amount),
        // which audibly colored mono/center and attenuated high frequencies even though
        // no high-frequency crossfeed was being added. Use the same low-frequency
        // isolated-channel cross-ear ratio while preserving mono and direct highs.
        let cross = s.crossfeed / (1.0 + s.crossfeed);
        let low_delta = self.low[1] - self.low[0];
        self.music
            .process([v[0] + cross * low_delta, v[1] - cross * low_delta])
            .map(|sample| sample * static_makeup)
    }
    fn adaptive_reduction_db(&self) -> f64 {
        self.music.adaptive_reduction_db()
    }
}

fn static_level_match_gain(settings: &Settings) -> f64 {
    if !settings.level_match {
        return 1.0;
    }
    const MAX_STATIC_MAKEUP: f64 = 31.622_776_601_683_793; // +30 dB
    (settings.level_target_gain / settings.gain.max(1e-12)).clamp(1.0, MAX_STATIC_MAKEUP)
}

fn perceptual_match_power(
    frame: [f64; 2],
    states: &mut [[crate::dsp::tone::State; 2]; 2],
    settings: &Settings,
) -> f64 {
    let raw_power = (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
    let mut weighted_power = 0.0;
    for channel in 0..2 {
        let pre = states[channel][0].process(frame[channel], settings.match_pre_filter);
        let weighted = states[channel][1].process(pre, settings.match_rlb_filter);
        weighted_power += weighted * weighted * 0.5;
    }
    // Pure RMS can sound mismatched after strong spectral shaping, while a pure K-weighted
    // detector can over-correct broadband electrical level. Bias toward BS.1770 weighting but
    // retain enough linear power to keep ordinary A/B and explicit-preamp behavior stable.
    raw_power * 0.30 + weighted_power * 0.70
}

pub struct Processor {
    active: Chain,
    next: Chain,
    remaining: u32,
    total: u32,
    limiter_gain: f64,
    match_reference_power: f64,
    match_output_power: f64,
    match_reference_filter: [[crate::dsp::tone::State; 2]; 2],
    match_output_filter: [[crate::dsp::tone::State; 2]; 2],
    match_gain: f64,
    match_transition_from: f64,
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
            match_reference_power: 0.0,
            match_output_power: 0.0,
            match_reference_filter: [[crate::dsp::tone::State::default(); 2]; 2],
            match_output_filter: [[crate::dsp::tone::State::default(); 2]; 2],
            match_gain: 1.0,
            match_transition_from: 1.0,
            pending: None,
        }
    }
    pub fn update(&mut self, settings: Settings) {
        // Entire chains are crossfaded; interpolating raw IIR coefficients is avoided.
        if self.remaining > 0 {
            if settings == self.next.settings {
                // Re-selecting the in-flight target cancels any older queued third target.
                self.pending = None;
                return;
            }
            if settings == self.active.settings {
                // A fast A→B→A reversal should not force the obsolete B target to finish
                // before returning. Reverse the existing crossfade at the exact same mix
                // point so the waveform stays continuous and the remaining time is only
                // the distance already travelled toward B.
                std::mem::swap(&mut self.active, &mut self.next);
                self.remaining = self.total.saturating_sub(self.remaining).max(1);
                std::mem::swap(&mut self.match_transition_from, &mut self.match_gain);
                self.match_reference_power = 0.0;
                self.match_output_power = 0.0;
                self.match_reference_filter = [[crate::dsp::tone::State::default(); 2]; 2];
                self.match_output_filter = [[crate::dsp::tone::State::default(); 2]; 2];
                self.pending = None;
                return;
            }
            self.pending = Some(settings);
            return;
        }
        if settings == self.active.settings {
            return;
        }
        // Residual loudness gain belongs to the measured signal path. Keep the old
        // value only as the first crossfade endpoint; fresh meters learn the new
        // blended path instead of dragging an opposite spectral profile's gain along.
        self.match_transition_from = self.match_gain;
        self.match_gain = 1.0;
        self.match_reference_power = 0.0;
        self.match_output_power = 0.0;
        self.match_reference_filter = [[crate::dsp::tone::State::default(); 2]; 2];
        self.match_output_filter = [[crate::dsp::tone::State::default(); 2]; 2];
        self.next = self.active.retune(settings);
        self.total = settings.transition_frames.max(1);
        self.remaining = self.total;
    }
    /// A discontinuity starts new filter history without reallocating, changing
    /// settings, forgetting queued transitions, or releasing limiter attenuation.
    pub(crate) fn reset_history(&mut self) {
        self.active = Chain::new(self.active.settings);
        self.next = Chain::new(self.next.settings);
        self.match_reference_power = 0.0;
        self.match_output_power = 0.0;
        self.match_reference_filter = [[crate::dsp::tone::State::default(); 2]; 2];
        self.match_output_filter = [[crate::dsp::tone::State::default(); 2]; 2];
        self.match_gain = 1.0;
        self.match_transition_from = 1.0;
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
    fn level_match_blend_at(&self, t: f64) -> f64 {
        let active = if self.active.settings.level_match {
            1.0
        } else {
            0.0
        };
        if self.remaining == 0 {
            return active;
        }
        let next = if self.next.settings.level_match {
            1.0
        } else {
            0.0
        };
        active * (1.0 - t) + next * t
    }
    fn level_match_blend(&self) -> f64 {
        let t = if self.remaining == 0 {
            1.0
        } else {
            1.0 - self.remaining as f64 / self.total as f64
        };
        self.level_match_blend_at(t)
    }
    pub(crate) fn settings_pending(&self) -> bool {
        self.remaining > 0 || self.pending.is_some()
    }
    pub fn level_match_makeup_db(&self) -> f64 {
        let t = if self.remaining == 0 {
            0.0
        } else {
            1.0 - self.remaining as f64 / self.total as f64
        };
        let static_makeup = static_level_match_gain(&self.active.settings) * (1.0 - t)
            + static_level_match_gain(&self.next.settings) * t;
        let transition_gain = if self.remaining == 0 {
            self.match_gain
        } else {
            self.match_transition_from * (1.0 - t) + self.match_gain * t
        };
        let residual_makeup = 1.0 + (transition_gain - 1.0) * self.level_match_blend();
        20.0 * (static_makeup * residual_makeup).max(1e-12).log10()
    }
    pub fn limiter_reduction_db(&self) -> f64 {
        -20.0 * self.limiter_gain.clamp(1e-12, 1.0).log10()
    }
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let transitioning = self.remaining > 0;
        // Each processed transition frame consumes one step. The last transition frame
        // must be audibly at t=1.0 before settings_pending() can become false.
        let transition_t = if self.remaining == 0 {
            1.0
        } else {
            1.0 - self.remaining.saturating_sub(1) as f64 / self.total as f64
        };
        let match_blend = if transitioning {
            self.level_match_blend_at(transition_t)
        } else {
            self.level_match_blend()
        };
        let active_settings = self.active.settings;
        let mut y = self.active.frame(input);
        let mut measurement = y;
        let mut measurement_settings = active_settings;
        let mut pending_after_frame = None;
        if self.remaining > 0 {
            let other = self.next.frame(input);
            let t = transition_t;
            // Warm the residual matcher against the incoming chain itself. Measuring
            // the temporary old/new mixture biases the detector toward the outgoing
            // spectrum and carries stale attenuation into the completed retune.
            measurement = other;
            measurement_settings = self.next.settings;
            for c in 0..2 {
                y[c] = y[c] * (1.0 - t) + other[c] * t;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.active = self.next;
                pending_after_frame = self.pending.take();
            }
        }
        if y.iter().any(|v| !v.is_finite()) {
            y = [0.0; 2];
        }
        // Compare against the real pre-Maris program at the user's requested preamp.
        // Static EQ/correction reserve stays in front of the filters, then measured settled
        // loudness may recover that reserve. The linked limiter below remains authoritative
        // for actual sample-peak safety.
        let settings = measurement_settings;
        let balance_gains = settings.music.level_reference_gains();
        let reference = std::array::from_fn(|channel| {
            let sample = input[channel];
            if sample.is_finite() {
                (sample as f64).clamp(-16.0, 16.0)
                    * settings.level_target_gain
                    * balance_gains[channel]
            } else {
                0.0
            }
        });
        if settings.level_match {
            let reference_power =
                perceptual_match_power(reference, &mut self.match_reference_filter, &settings);
            let output_power =
                perceptual_match_power(measurement, &mut self.match_output_filter, &settings);
            self.match_reference_power = settings.match_meter * self.match_reference_power
                + (1.0 - settings.match_meter) * reference_power;
            self.match_output_power = settings.match_meter * self.match_output_power
                + (1.0 - settings.match_meter) * output_power;
            let desired_match = if reference_power <= 1e-12 && output_power <= 1e-12 {
                // Digital silence is a content boundary, not evidence that the next program
                // needs the previous program's residual spectral makeup. Let the residual
                // gain return smoothly to unity while static safety-reserve recovery remains
                // active in each chain.
                1.0
            } else if self.match_reference_power > 1e-10 && self.match_output_power > 1e-10 {
                // Static reserve is already recovered per chain. Bound only the remaining
                // program-dependent correction so a spectral null cannot become unbounded gain.
                const MAX_RESIDUAL_MAKEUP: f64 = 3.981_071_705_534_972_2; // +12 dB
                (self.match_reference_power / self.match_output_power)
                    .sqrt()
                    .clamp(0.25, MAX_RESIDUAL_MAKEUP)
            } else {
                1.0
            };
            if transitioning {
                self.match_gain = settings.match_transition_slew * self.match_gain
                    + (1.0 - settings.match_transition_slew) * desired_match;
            } else {
                self.match_gain = settings.match_slew * self.match_gain
                    + (1.0 - settings.match_slew) * desired_match;
            }
        } else if match_blend <= f64::EPSILON {
            // Once the settings crossfade has fully removed Level Match, clear the program
            // meter so a later re-enable cannot reuse stale history. During the crossfade,
            // keep the old meter but fade its residual gain to unity with the same envelope.
            self.match_reference_power = 0.0;
            self.match_output_power = 0.0;
            self.match_reference_filter = [[crate::dsp::tone::State::default(); 2]; 2];
            self.match_output_filter = [[crate::dsp::tone::State::default(); 2]; 2];
            self.match_gain = 1.0;
        }
        let transition_gain = if transitioning {
            self.match_transition_from * (1.0 - transition_t) + self.match_gain * transition_t
        } else {
            self.match_gain
        };
        let applied_match_gain = 1.0 + (transition_gain - 1.0) * match_blend;
        y = y.map(|sample| sample * applied_match_gain);

        let peak = y[0].abs().max(y[1].abs());
        const CEILING: f64 = 0.8912509381337456; // -1 dBFS sample peak, not true peak or SPL.
        let wanted = if peak > CEILING { CEILING / peak } else { 1.0 };
        if wanted < self.limiter_gain {
            self.limiter_gain = wanted;
        } else {
            self.limiter_gain = self.active.settings.release * self.limiter_gain
                + (1.0 - self.active.settings.release) * wanted;
        }
        let output = y.map(|v| (v * self.limiter_gain).clamp(-CEILING, CEILING) as f32);
        if let Some(settings) = pending_after_frame {
            self.update(settings);
        }
        output
    }
}

#[cfg(test)]
#[path = "../../tests/unit/dsp_retune.rs"]
mod retune_tests;
