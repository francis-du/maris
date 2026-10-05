//! Fixed-size mixer coefficients. Compilation allocates only on the control thread.
use super::{MixerConfig, BUS_COUNT, CEILING, MAX_STRIPS};
use anyhow::{ensure, Result};

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    routes: [[[f64; 2]; BUS_COUNT]; MAX_STRIPS],
    triggers: [bool; MAX_STRIPS],
    ducks: [bool; MAX_STRIPS],
    count: usize,
    threshold: f64,
    attenuation: f64,
    attack: f64,
    release: f64,
    ramp_frames: u32,
}
impl Settings {
    pub fn compile(config: &MixerConfig, rate: u32) -> Result<Self> {
        config.validate()?;
        ensure!(
            (44100..=192000).contains(&rate),
            "Mixer supports 44100..192000 Hz"
        );
        let mut result = Self {
            routes: [[[0.0; 2]; BUS_COUNT]; MAX_STRIPS],
            triggers: [false; MAX_STRIPS],
            ducks: [false; MAX_STRIPS],
            count: config.strips.len(),
            threshold: 10.0_f64.powf(config.ducking.threshold_dbfs / 20.0),
            attenuation: if config.ducking.enabled {
                10.0_f64.powf(-config.ducking.attenuation_db / 20.0)
            } else {
                1.0
            },
            attack: (-1.0 / (rate as f64 * config.ducking.attack_ms * 0.001)).exp(),
            release: (-1.0 / (rate as f64 * config.ducking.release_ms * 0.001)).exp(),
            ramp_frames: (rate / 40).max(1),
        };
        let solo = config.strips.iter().any(|strip| strip.solo);
        for (index, strip) in config.strips.iter().enumerate() {
            let audible = !strip.mute && (!solo || strip.solo);
            result.triggers[index] = audible && strip.voice_trigger;
            result.ducks[index] = strip.duck_target;
            let angle = (strip.pan + 1.0) * std::f64::consts::FRAC_PI_4;
            let pan = [
                angle.cos() * std::f64::consts::SQRT_2,
                angle.sin() * std::f64::consts::SQRT_2,
            ];
            for (bus, target) in config.buses.iter().enumerate() {
                let gain = if audible && !target.mute {
                    10.0_f64.powf((strip.gain_db + target.gain_db) / 20.0) * strip.sends[bus]
                } else {
                    0.0
                };
                result.routes[index][bus] = pan.map(|v| v * gain);
            }
        }
        Ok(result)
    }
}

#[derive(Clone)]
pub struct Processor {
    current: Settings,
    target: Settings,
    remaining: u32,
    duck_gain: f64,
    duck_mix: [f64; MAX_STRIPS],
}
impl Processor {
    pub fn new(config: MixerConfig, rate: u32) -> Result<Self> {
        Ok(Self::compiled(Settings::compile(&config, rate)?))
    }
    pub fn compiled(settings: Settings) -> Self {
        Self {
            current: settings,
            target: settings,
            remaining: 0,
            duck_gain: 1.0,
            duck_mix: settings.ducks.map(|duck| if duck { 1.0 } else { 0.0 }),
        }
    }
    /// The caller compiles and validates before queueing. No ownership or allocation crosses here.
    pub fn update(&mut self, settings: Settings) {
        if settings == self.target {
            return;
        }
        self.target = settings;
        self.remaining = settings.ramp_frames;
    }
    pub(crate) fn settings_pending(&self) -> bool {
        self.remaining > 0
    }
    pub fn process(&mut self, inputs: &[[f32; 2]]) -> [[f32; 2]; BUS_COUNT] {
        self.process_unlimited(inputs)
            .map(|frame| frame.map(|v| v.clamp(-CEILING, CEILING) as f32))
    }
    /// Native buses feed their own linked output limiter after device correction.
    pub(crate) fn process_unlimited(&mut self, inputs: &[[f32; 2]]) -> [[f64; 2]; BUS_COUNT] {
        if self.remaining > 0 {
            let fraction = 1.0 / self.remaining as f64;
            for (now, target) in self.current.routes.iter_mut().zip(self.target.routes) {
                for (now, target) in now.iter_mut().zip(target) {
                    for (now, target) in now.iter_mut().zip(target) {
                        *now += (target - *now) * fraction;
                    }
                }
            }
            for (mix, target) in self.duck_mix.iter_mut().zip(self.target.ducks) {
                let target = if target { 1.0 } else { 0.0 };
                *mix += (target - *mix) * fraction;
            }
            self.current.count = self.current.count.max(self.target.count);
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
                self.duck_mix = self.target.ducks.map(|duck| if duck { 1.0 } else { 0.0 });
            }
        }
        let mut trigger = 0.0_f64;
        let mut clean = [[0.0; 2]; MAX_STRIPS];
        for (i, frame) in inputs.iter().take(MAX_STRIPS).enumerate() {
            clean[i] = frame.map(|v| {
                if v.is_finite() {
                    (v as f64).clamp(-16.0, 16.0)
                } else {
                    0.0
                }
            });
            if self.target.triggers[i] {
                trigger = trigger.max(clean[i][0].abs().max(clean[i][1].abs()));
            }
        }
        let target = if trigger >= self.target.threshold {
            self.target.attenuation
        } else {
            1.0
        };
        let coefficient = if target < self.duck_gain {
            self.target.attack
        } else {
            self.target.release
        };
        self.duck_gain = coefficient * self.duck_gain + (1.0 - coefficient) * target;
        let mut buses = [[0.0; 2]; BUS_COUNT];
        for (i, frame) in clean.iter().enumerate().take(self.current.count) {
            let duck = 1.0 + (self.duck_gain - 1.0) * self.duck_mix[i];
            for (bus, out) in buses.iter_mut().enumerate() {
                for channel in 0..2 {
                    out[channel] += frame[channel] * self.current.routes[i][bus][channel] * duck;
                }
            }
        }
        buses
    }
}
