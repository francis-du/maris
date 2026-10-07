//! Device correction and listening preferences are separate from the original 10-band profile.
use crate::dsp::tone::{Biquad, Filter, Kind, State};
use anyhow::{ensure, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_FILTERS: usize = 20;
pub const PRESETS: [&str; 19] = [
    "natural",
    "warm",
    "vocal",
    "detail",
    "soft",
    "night",
    "focus",
    "long-listening",
    "dialogue",
    "cinema",
    "night-dialogue",
    "acoustic-listening",
    "orchestral",
    "tight-bass",
    "game-clarity",
    "small-speakers",
    "surround-360",
    "cinema-360",
    "stereo-focus",
];
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MusicProfile {
    pub bass_db: f64,
    pub presence_db: f64,
    pub air_db: f64,
    pub softness: f64,
    pub intensity: f64,
    pub width: f64,
    pub balance: f64,
    pub virtual_surround: f64,
    pub stereo_focus: f64,
    pub highpass_hz: Option<f64>,
    pub correction: Vec<Filter>,
    pub correction_preamp_db: f64,
    pub correction_source: Option<String>,
    pub compressor: Compressor,
    pub adaptive: AdaptiveEq,
    pub bass_assist: BassAssist,
    pub enabled: bool,
    pub reference: bool,
    pub level_match: bool,
}
impl Default for MusicProfile {
    fn default() -> Self {
        Self {
            bass_db: 0.0,
            presence_db: 0.0,
            air_db: 0.0,
            softness: 0.0,
            intensity: 1.0,
            width: 1.0,
            balance: 0.0,
            virtual_surround: 0.0,
            stereo_focus: 0.0,
            highpass_hz: None,
            correction: Vec::new(),
            correction_preamp_db: 0.0,
            correction_source: None,
            compressor: Compressor::default(),
            adaptive: AdaptiveEq::default(),
            bass_assist: BassAssist::default(),
            enabled: true,
            reference: false,
            level_match: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AdaptiveEq {
    pub enabled: bool,
    pub strength: f64,
}
impl Default for AdaptiveEq {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.45,
        }
    }
}
impl AdaptiveEq {
    pub fn validate(self) -> Result<()> {
        bound(self.strength, 0.0, 1.0, "adaptive.strength")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BassAssist {
    pub enabled: bool,
    pub amount: f64,
}
impl Default for BassAssist {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 0.35,
        }
    }
}
impl BassAssist {
    pub fn validate(self) -> Result<()> {
        bound(self.amount, 0.0, 1.0, "bass_assist.amount")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Compressor {
    pub enabled: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub knee_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
}
impl Default for Compressor {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 20.0,
            release_ms: 200.0,
        }
    }
}
pub fn bound(value: f64, lo: f64, hi: f64, name: &str) -> Result<()> {
    ensure!(
        value.is_finite() && (lo..=hi).contains(&value),
        "{name} must be finite and in [{lo}, {hi}]"
    );
    Ok(())
}
impl Compressor {
    pub fn validate(self) -> Result<()> {
        bound(self.threshold_db, -48.0, 0.0, "threshold_db")?;
        bound(self.ratio, 1.0, 8.0, "ratio")?;
        bound(self.knee_db, 0.0, 24.0, "knee_db")?;
        bound(self.attack_ms, 1.0, 200.0, "attack_ms")?;
        bound(self.release_ms, 20.0, 2000.0, "release_ms")
    }
}
impl MusicProfile {
    pub fn output_switch_baseline(&self) -> Self {
        let mut baseline = self.clone();
        baseline.bass_db = 0.0;
        baseline.presence_db = 0.0;
        baseline.air_db = 0.0;
        baseline.softness = 0.0;
        baseline.intensity = 1.0;
        baseline.width = 1.0;
        baseline.balance = 0.0;
        baseline.virtual_surround = 0.0;
        baseline.stereo_focus = 0.0;
        baseline.compressor.enabled = false;
        baseline.adaptive.enabled = false;
        baseline.bass_assist.enabled = false;
        baseline.reference = false;
        // Loudness matching is not a subjective tone control. Preserve the user's
        // choice so correction/headroom reserve does not make a new output start
        // several dB quieter than the pre-Maris signal.
        baseline.level_match = self.level_match;
        baseline
    }

    pub fn validate(&self) -> Result<()> {
        bound(self.bass_db, -6.0, 6.0, "bass_db")?;
        bound(self.presence_db, -3.0, 3.0, "presence_db")?;
        bound(self.air_db, -3.0, 3.0, "air_db")?;
        bound(self.softness, 0.0, 1.0, "softness")?;
        bound(self.intensity, 0.0, 1.0, "intensity")?;
        bound(self.width, 0.0, 1.5, "width")?;
        bound(self.balance, -1.0, 1.0, "balance")?;
        bound(self.virtual_surround, 0.0, 1.0, "virtual_surround")?;
        bound(self.stereo_focus, 0.0, 1.0, "stereo_focus")?;
        bound(
            self.correction_preamp_db,
            -30.0,
            0.0,
            "correction_preamp_db",
        )?;
        if let Some(hz) = self.highpass_hz {
            bound(hz, 20.0, 200.0, "highpass_hz")?;
        }
        ensure!(
            self.correction.len() <= 16,
            "A device correction supports at most 16 filters"
        );
        for filter in &self.correction {
            filter.validate()?;
        }
        if let Some(source) = &self.correction_source {
            ensure!(
                !source.is_empty() && source.len() <= 1024 && !source.chars().any(char::is_control),
                "Invalid correction provenance"
            );
        }
        self.compressor.validate()?;
        self.adaptive.validate()?;
        self.bass_assist.validate()
    }
    pub fn preset(name: &str) -> Result<Self> {
        let mut p = Self::default();
        match name {
            "natural" => {}
            "warm" => {
                p.bass_db = 1.5;
                p.softness = 0.25;
            }
            "vocal" => {
                p.presence_db = 1.5;
                p.bass_db = -0.5;
            }
            "detail" => {
                p.presence_db = 0.5;
                p.air_db = 1.0;
            }
            "soft" => {
                p.softness = 0.5;
            }
            "night" => {
                p.compressor.enabled = true;
                p.compressor.threshold_db = -24.0;
                p.compressor.ratio = 2.5;
            }
            _ if crate::presets::scenes::find(name).is_some() => {
                return crate::presets::scenes::profile(name)
            }
            _ => anyhow::bail!("Unknown music preset. Available: {}", PRESETS.join(", ")),
        }
        Ok(p)
    }
    pub fn compile(&self, rate: u32) -> Result<Settings> {
        self.validate()?;
        ensure!(
            (44100..=192000).contains(&rate),
            "Unsupported music processing rate"
        );
        let mut filters = [Biquad::default(); MAX_FILTERS];
        let mut count = 0;

        let additions = [
            Filter {
                kind: Kind::LowShelf,
                frequency_hz: 100.0,
                gain_db: self.bass_db * self.intensity,
                q: std::f64::consts::FRAC_1_SQRT_2,
            },
            Filter {
                kind: Kind::Peak,
                frequency_hz: 2300.0,
                gain_db: self.presence_db * self.intensity,
                q: 0.7,
            },
            Filter {
                kind: Kind::HighShelf,
                frequency_hz: 8000.0,
                gain_db: self.air_db * self.intensity,
                q: std::f64::consts::FRAC_1_SQRT_2,
            },
        ];
        for filter in self.correction.iter().chain(additions.iter()) {
            filters[count] = filter.compile(rate)?;
            count += 1;
        }
        if let Some(hz) = self.highpass_hz {
            filters[count] = Filter {
                kind: Kind::HighPass,
                frequency_hz: hz,
                gain_db: 0.0,
                q: std::f64::consts::FRAC_1_SQRT_2,
            }
            .compile(rate)?;
            count += 1;
        }
        let peak = crate::dsp::tone::peak_response(rate, |hz| {
            filters[..count]
                .iter()
                .map(|c| c.response_db(hz, rate))
                .sum()
        });
        let headroom = if peak > 0.01 { -peak - 0.5 } else { 0.0 };
        Ok(Settings {
            filters,
            count,
            gain: if self.enabled {
                10.0_f64.powf(self.correction_preamp_db.min(headroom) / 20.0)
            } else {
                1.0
            },
            width: self.width,
            balance: self.balance,
            virtual_surround: self.virtual_surround,
            stereo_focus: self.stereo_focus,
            surround_highpass: Filter {
                kind: Kind::HighPass,
                frequency_hz: 320.0,
                gain_db: 0.0,
                q: std::f64::consts::FRAC_1_SQRT_2,
            }
            .compile(rate)?,
            surround_allpass_a: Biquad::allpass(700.0, 0.7, rate)?,
            surround_allpass_b: Biquad::allpass(3200.0, 0.8, rate)?,
            compressor: self.compressor,
            attack: (-1.0 / (rate as f64 * self.compressor.attack_ms * 0.001)).exp(),
            release: (-1.0 / (rate as f64 * self.compressor.release_ms * 0.001)).exp(),
            meter: (-1.0 / (rate as f64 * 1.5)).exp(),
            program_meter: (-1.0 / (rate as f64 * 0.4)).exp(),
            adaptive: adaptive_bands(self.adaptive, self.softness, self.intensity, rate)?,
            bass_assist: bass_assist_settings(self.bass_assist, rate)?,
            enabled: self.enabled,
            reference: self.reference,
            level_match: self.level_match,
        })
    }
}
const ADAPTIVE_BANDS: usize = 3;

#[derive(Clone, Copy, PartialEq)]
struct AdaptiveBand {
    detector: Biquad,
    cut: Biquad,
    max_reduction_db: f64,
    threshold_db: f64,
    attack: f64,
    release: f64,
}
fn adaptive_band(
    rate: u32,
    frequency_hz: f64,
    q: f64,
    max_reduction_db: f64,
    threshold_db: f64,
    attack_ms: f64,
    release_ms: f64,
) -> Result<AdaptiveBand> {
    Ok(AdaptiveBand {
        detector: Biquad::bandpass(frequency_hz, q, rate)?,
        cut: Filter {
            kind: Kind::Peak,
            frequency_hz,
            gain_db: -max_reduction_db,
            q,
        }
        .compile(rate)?,
        max_reduction_db,
        threshold_db,
        attack: (-1.0 / (rate as f64 * attack_ms * 0.001)).exp(),
        release: (-1.0 / (rate as f64 * release_ms * 0.001)).exp(),
    })
}
fn adaptive_bands(
    config: AdaptiveEq,
    softness: f64,
    intensity: f64,
    rate: u32,
) -> Result<[AdaptiveBand; ADAPTIVE_BANDS]> {
    let strength = if config.enabled {
        config.strength * intensity
    } else {
        0.0
    };
    Ok([
        adaptive_band(rate, 95.0, 0.75, 1.5 * strength, -7.0, 60.0, 350.0)?,
        adaptive_band(rate, 3200.0, 1.25, 1.5 * strength, -9.0, 18.0, 180.0)?,
        adaptive_band(
            rate,
            8500.0,
            1.5,
            (1.0 + 2.0 * softness) * strength,
            -12.0 - 3.0 * softness,
            6.0,
            110.0,
        )?,
    ])
}

#[derive(Clone, Copy, PartialEq)]
struct BassAssistSettings {
    enabled: bool,
    amount: f64,
    source_highpass: Biquad,
    source_lowpass: Biquad,
    harmonic_highpass: Biquad,
    harmonic_lowpass: Biquad,
}
fn bass_assist_settings(config: BassAssist, rate: u32) -> Result<BassAssistSettings> {
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let compile = |kind, frequency_hz| {
        Filter {
            kind,
            frequency_hz,
            gain_db: 0.0,
            q,
        }
        .compile(rate)
    };
    Ok(BassAssistSettings {
        enabled: config.enabled && config.amount > 0.0,
        amount: config.amount,
        source_highpass: compile(Kind::HighPass, 28.0)?,
        source_lowpass: compile(Kind::LowPass, 160.0)?,
        harmonic_highpass: compile(Kind::HighPass, 140.0)?,
        harmonic_lowpass: compile(Kind::LowPass, 650.0)?,
    })
}

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    filters: [Biquad; MAX_FILTERS],
    count: usize,
    pub gain: f64,
    width: f64,
    balance: f64,
    virtual_surround: f64,
    stereo_focus: f64,
    surround_highpass: Biquad,
    surround_allpass_a: Biquad,
    surround_allpass_b: Biquad,
    compressor: Compressor,
    attack: f64,
    release: f64,
    meter: f64,
    program_meter: f64,
    adaptive: [AdaptiveBand; ADAPTIVE_BANDS],
    bass_assist: BassAssistSettings,
    enabled: bool,
    reference: bool,
    level_match: bool,
}
impl Settings {
    pub fn response_db(&self, hz: f64, rate: u32) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        self.filters[..self.count]
            .iter()
            .map(|c| c.response_db(hz, rate))
            .sum()
    }

    pub(crate) fn level_reference_gains(&self) -> [f64; 2] {
        if !self.enabled || self.reference {
            return [1.0, 1.0];
        }
        [1.0 - self.balance.max(0.0), 1.0 + self.balance.min(0.0)]
    }

    pub(crate) fn match_detector_needs_warmup(&self, other: &Self) -> bool {
        // Spatial transfer changes can rotate/decorrelate a single frame enough that a fresh
        // K-weighted power ratio is not representative. Dynamics/EQ toggles already have
        // their own state ramps and must not be held in repeated detector cold starts.
        self.virtual_surround != other.virtual_surround
            || self.stereo_focus != other.stereo_focus
            || self.width != other.width
    }
}

#[derive(Clone, Copy)]
pub struct Processor {
    settings: Settings,
    states: [[State; MAX_FILTERS]; 2],
    adaptive_detector: [[State; ADAPTIVE_BANDS]; 2],
    adaptive_cut: [[State; ADAPTIVE_BANDS]; 2],
    adaptive_power: [f64; ADAPTIVE_BANDS],
    adaptive_reduction_db: [f64; ADAPTIVE_BANDS],
    bass_source_highpass: [State; 2],
    bass_source_lowpass: [State; 2],
    bass_harmonic_highpass: [State; 2],
    bass_harmonic_lowpass: [State; 2],
    surround_highpass: State,
    surround_allpass_a: State,
    surround_allpass_b: State,
    program_power: f64,
    reduction_db: f64,
    dry_power: f64,
    wet_power: f64,
}
impl Processor {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            states: [[State::default(); MAX_FILTERS]; 2],
            adaptive_detector: [[State::default(); ADAPTIVE_BANDS]; 2],
            adaptive_cut: [[State::default(); ADAPTIVE_BANDS]; 2],
            adaptive_power: [0.0; ADAPTIVE_BANDS],
            adaptive_reduction_db: [0.0; ADAPTIVE_BANDS],
            bass_source_highpass: [State::default(); 2],
            bass_source_lowpass: [State::default(); 2],
            bass_harmonic_highpass: [State::default(); 2],
            bass_harmonic_lowpass: [State::default(); 2],
            surround_highpass: State::default(),
            surround_allpass_a: State::default(),
            surround_allpass_b: State::default(),
            program_power: 0.0,
            reduction_db: 0.0,
            dry_power: 0.0,
            wet_power: 0.0,
        }
    }
    pub fn retune(&self, settings: Settings) -> Self {
        let mut next = Self::new(settings);
        // The dry/wet ratio is valid as long as the actual signal path is unchanged.
        // Reference/Level Match toggles only choose which already-computed branch is heard;
        // throwing away the 1.5 s meter history there creates a transient A/B level wobble.
        // Any change that alters the wet path must still reset the ratio.
        let state_continuous = self.settings.enabled && settings.enabled;
        let compressor_feed_unchanged = state_continuous
            && self.settings.gain == settings.gain
            && self.settings.count == settings.count
            && self.settings.filters == settings.filters
            && self.settings.adaptive == settings.adaptive
            && self.settings.bass_assist == settings.bass_assist
            && self.settings.width == settings.width
            && self.settings.balance == settings.balance
            && self.settings.virtual_surround == settings.virtual_surround
            && self.settings.stereo_focus == settings.stereo_focus
            && self.settings.surround_highpass == settings.surround_highpass
            && self.settings.surround_allpass_a == settings.surround_allpass_a
            && self.settings.surround_allpass_b == settings.surround_allpass_b
            && self.settings.attack == settings.attack
            && self.settings.release == settings.release;
        let signal_path_unchanged =
            self.settings.compressor == settings.compressor && compressor_feed_unchanged;
        if signal_path_unchanged {
            next.dry_power = self.dry_power;
            next.wet_power = self.wet_power;
            next.reduction_db = self.reduction_db;
        }

        // Dynamic-EQ detection runs from `dry`, while its cut filters run on the
        // already tone-shaped `wet` path. Preserve those histories independently:
        // a pure A/B/reference change may keep both, a gain change invalidates the
        // detector history, and an upstream tone/correction change invalidates only
        // the cut-filter state so old IIR energy cannot leak into the new curve.
        let adaptive_detector_feed_unchanged = state_continuous
            && self.settings.gain == settings.gain
            && self.settings.adaptive == settings.adaptive;
        let adaptive_cut_feed_unchanged = adaptive_detector_feed_unchanged
            && self.settings.count == settings.count
            && self.settings.filters == settings.filters;
        if adaptive_detector_feed_unchanged {
            next.program_power = self.program_power;
        }
        if state_continuous {
            // Preserve only the unchanged prefix of the serial tone/correction
            // cascade. A matching downstream biquad still has stale state if an
            // earlier stage or the pre-filter gain changed its historical input.
            let mut filter_prefix_unchanged = self.settings.gain == settings.gain;
            for i in 0..MAX_FILTERS {
                filter_prefix_unchanged &= self.settings.filters[i] == settings.filters[i];
                if filter_prefix_unchanged {
                    for c in 0..2 {
                        next.states[c][i] = self.states[c][i];
                    }
                }
            }
            for c in 0..2 {
                for i in 0..ADAPTIVE_BANDS {
                    if adaptive_detector_feed_unchanged
                        && self.settings.adaptive[i] == settings.adaptive[i]
                    {
                        next.adaptive_detector[c][i] = self.adaptive_detector[c][i];
                    }
                    if adaptive_cut_feed_unchanged
                        && self.settings.adaptive[i] == settings.adaptive[i]
                    {
                        next.adaptive_cut[c][i] = self.adaptive_cut[c][i];
                    }
                }
            }
            if adaptive_detector_feed_unchanged {
                for i in 0..ADAPTIVE_BANDS {
                    if self.settings.adaptive[i] == settings.adaptive[i] {
                        next.adaptive_power[i] = self.adaptive_power[i];
                        next.adaptive_reduction_db[i] = self.adaptive_reduction_db[i];
                    }
                }
            }
        }
        if state_continuous
            && self.settings.gain == settings.gain
            && self.settings.bass_assist == settings.bass_assist
        {
            next.bass_source_highpass = self.bass_source_highpass;
            next.bass_source_lowpass = self.bass_source_lowpass;
            next.bass_harmonic_highpass = self.bass_harmonic_highpass;
            next.bass_harmonic_lowpass = self.bass_harmonic_lowpass;
        }
        let spatial_feed_unchanged = state_continuous
            && self.settings.gain == settings.gain
            && self.settings.count == settings.count
            && self.settings.filters == settings.filters
            && self.settings.adaptive == settings.adaptive
            && self.settings.bass_assist == settings.bass_assist;
        if spatial_feed_unchanged
            && self.settings.virtual_surround > 1e-9
            && settings.virtual_surround > 1e-9
            && self.settings.surround_highpass == settings.surround_highpass
            && self.settings.surround_allpass_a == settings.surround_allpass_a
            && self.settings.surround_allpass_b == settings.surround_allpass_b
        {
            next.surround_highpass = self.surround_highpass;
            next.surround_allpass_a = self.surround_allpass_a;
            next.surround_allpass_b = self.surround_allpass_b;
        }
        next
    }
    pub fn adaptive_reduction_db(&self) -> f64 {
        self.adaptive_reduction_db
            .iter()
            .copied()
            .fold(0.0_f64, f64::max)
    }
    fn adaptive(&mut self, dry: [f64; 2], mut wet: [f64; 2]) -> [f64; 2] {
        let s = self.settings;
        let program = (dry[0] * dry[0] + dry[1] * dry[1]) * 0.5;
        self.program_power =
            s.program_meter * self.program_power + (1.0 - s.program_meter) * program;
        if self.program_power < 1e-12 {
            self.adaptive_reduction_db = [0.0; ADAPTIVE_BANDS];
            return wet;
        }
        for band_index in 0..ADAPTIVE_BANDS {
            let band = s.adaptive[band_index];
            if band.max_reduction_db <= 1e-6 {
                self.adaptive_reduction_db[band_index] = 0.0;
                continue;
            }
            let mut detector_power = 0.0;
            for (channel, sample) in dry.iter().enumerate() {
                let detected =
                    self.adaptive_detector[channel][band_index].process(*sample, band.detector);
                detector_power += detected * detected * 0.5;
            }
            let coeff = if detector_power > self.adaptive_power[band_index] {
                band.attack
            } else {
                band.release
            };
            self.adaptive_power[band_index] =
                coeff * self.adaptive_power[band_index] + (1.0 - coeff) * detector_power;
            let relative_db = 10.0
                * ((self.adaptive_power[band_index] + 1e-15) / (self.program_power + 1e-15))
                    .log10();
            let reduction_db =
                ((relative_db - band.threshold_db).max(0.0) * 0.35).min(band.max_reduction_db);
            self.adaptive_reduction_db[band_index] = reduction_db;
            if reduction_db <= 1e-6 {
                continue;
            }
            let full = 10.0_f64.powf(-band.max_reduction_db / 20.0);
            let target = 10.0_f64.powf(-reduction_db / 20.0);
            let mix = ((target - 1.0) / (full - 1.0)).clamp(0.0, 1.0);
            for (channel, sample) in wet.iter_mut().enumerate() {
                let filtered = self.adaptive_cut[channel][band_index].process(*sample, band.cut);
                *sample += (filtered - *sample) * mix;
            }
        }
        wet
    }
    fn bass_assist(&mut self, dry: [f64; 2], mut wet: [f64; 2]) -> [f64; 2] {
        let settings = self.settings.bass_assist;
        if !settings.enabled || settings.amount <= 0.0 {
            return wet;
        }
        let low: [f64; 2] = std::array::from_fn(|channel| {
            let highpassed =
                self.bass_source_highpass[channel].process(dry[channel], settings.source_highpass);
            self.bass_source_lowpass[channel].process(highpassed, settings.source_lowpass)
        });
        let low_peak = low[0].abs().max(low[1].abs());
        let drive_scale = if low_peak > 1.5 { 1.5 / low_peak } else { 1.0 };
        let drive = low.map(|sample| sample * drive_scale);
        let linked_drive = drive[0].abs().max(drive[1].abs());
        let shape = if linked_drive > 1e-12 {
            let odd = (3.0 * linked_drive).tanh() / 3.0 - linked_drive;
            -1.6 * odd / linked_drive * settings.amount
        } else {
            0.0
        };
        // A shared nonlinear shape preserves the instantaneous stereo vector: mono stays mono,
        // pure Side stays Side, and fixed pan ratios do not drift as harmonic drive changes.
        // Clamp the entire vector with one scale so extreme input cannot bypass the original
        // 1.5 drive bound or alter the L/R proportion.
        for channel in 0..2 {
            let generated = drive[channel] * shape;
            let harmonics =
                self.bass_harmonic_highpass[channel].process(generated, settings.harmonic_highpass);
            let harmonics =
                self.bass_harmonic_lowpass[channel].process(harmonics, settings.harmonic_lowpass);
            wet[channel] += harmonics * 0.35;
        }
        wet
    }
    pub fn process(&mut self, input: [f64; 2]) -> [f64; 2] {
        let s = self.settings;
        if !s.enabled {
            return input.map(|v| if v.is_finite() { v } else { 0.0 });
        }
        let dry = input.map(|v| if v.is_finite() { v * s.gain } else { 0.0 });
        let mut wet = dry;
        for (channel, sample) in wet.iter_mut().enumerate() {
            for i in 0..s.count {
                *sample = self.states[channel][i].process(*sample, s.filters[i]);
            }
        }
        wet = self.adaptive(dry, wet);
        wet = self.bass_assist(dry, wet);
        let mid = (wet[0] + wet[1]) * 0.5;
        let mut side = (wet[0] - wet[1]) * 0.5;
        if s.virtual_surround > 1e-9 {
            // Always advance the spatial filter state so mono/silent passages
            // drain previous Side history instead of freezing it for later.
            let high = self.surround_highpass.process(side, s.surround_highpass);
            let phased = self.surround_allpass_b.process(
                self.surround_allpass_a.process(high, s.surround_allpass_a),
                s.surround_allpass_b,
            );
            // Do not inject the filter's decaying history into a true mono center.
            if side.abs() > 1e-15 {
                side += (phased - high) * s.virtual_surround;
            }
        }
        side *= (1.0 - 0.75 * s.stereo_focus) * s.width;
        // Preserve the Mid channel at unity while scaling/decorrelating Side.
        wet = [mid + side, mid - side];
        wet[0] *= 1.0 - s.balance.max(0.0);
        wet[1] *= 1.0 + s.balance.min(0.0);
        if s.compressor.enabled {
            let level = 20.0 * wet[0].abs().max(wet[1].abs()).max(1e-12).log10();
            let x = level - s.compressor.threshold_db;
            let k = s.compressor.knee_db;
            let amount = if k > 0.0 && x.abs() < k * 0.5 {
                (x + k * 0.5).powi(2) / (2.0 * k)
            } else {
                x.max(0.0)
            };
            let target = (1.0 - 1.0 / s.compressor.ratio) * amount;
            let coeff = if target > self.reduction_db {
                s.attack
            } else {
                s.release
            };
            self.reduction_db = coeff * self.reduction_db + (1.0 - coeff) * target;
            let gain = 10.0_f64.powf(-self.reduction_db / 20.0);
            wet = wet.map(|v| v * gain);
        }
        let dry_power = (dry[0] * dry[0] + dry[1] * dry[1]) * 0.5;
        let wet_power = (wet[0] * wet[0] + wet[1] * wet[1]) * 0.5;
        self.dry_power = s.meter * self.dry_power + (1.0 - s.meter) * dry_power;
        self.wet_power = s.meter * self.wet_power + (1.0 - s.meter) * wet_power;
        let (dg, wg) = if s.level_match && self.dry_power > 1e-10 && self.wet_power > 1e-10 {
            (
                (self.wet_power / self.dry_power).sqrt().min(1.0),
                (self.dry_power / self.wet_power).sqrt().min(1.0),
            )
        } else {
            (1.0, 1.0)
        };
        if s.reference {
            dry.map(|v| v * dg)
        } else {
            wet.map(|v| v * wg)
        }
    }
}
