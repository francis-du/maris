use anyhow::{bail, ensure, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FREQUENCIES: [f64; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
pub const PRESETS: [&str; 6] = ["flat", "warm", "vocal", "bass", "soft", "clarity"];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Band {
    #[schemars(range(min = 20.0, max = 20000.0))]
    pub frequency_hz: f64,
    #[schemars(range(min = -24.0, max = 24.0))]
    pub gain_db: f64,
    #[schemars(range(min = 0.2, max = 5.0))]
    pub q: f64,
    /// Optional source bandwidth in octaves. When present, the DSP derives Q at the negotiated sample rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.05, max = 4.0))]
    pub bandwidth_octaves: Option<f64>,
}
impl Band {
    pub fn q_at(self, rate: u32) -> f64 {
        let Some(bandwidth) = self.bandwidth_octaves else {
            return self.q;
        };
        let w0 = std::f64::consts::TAU * self.frequency_hz / rate as f64;
        let warping = if w0.sin().abs() > 1e-12 {
            w0 / w0.sin()
        } else {
            1.0
        };
        1.0 / (2.0 * (std::f64::consts::LN_2 * 0.5 * bandwidth * warping).sinh())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[schemars(length(min = 1, max = 80))]
    pub name: String,
    #[schemars(range(min = -30.0, max = 0.0))]
    pub preamp_db: f64,
    /// Optional preset safety margin. When present, cascade headroom is retained during bypass/A-B comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 6.0))]
    pub safety_margin_db: Option<f64>,
    pub bands: [Band; 10],
    #[schemars(range(min = 0.0, max = 0.3))]
    pub crossfeed: f64,
    #[schemars(range(min = 0.0, max = 1.5))]
    pub stereo_width: f64,
    pub bypass: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "flat".into(),
            preamp_db: 0.0,
            safety_margin_db: None,
            bands: FREQUENCIES.map(|frequency_hz| Band {
                frequency_hz,
                gain_db: 0.0,
                q: 1.0,
                bandwidth_octaves: None,
            }),
            crossfeed: 0.0,
            stereo_width: 1.0,
            bypass: false,
        }
    }
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 80
                && !self.name.chars().any(char::is_control),
            "Profile name must contain 1..80 bytes without control characters"
        );
        check(self.preamp_db, -30.0, 0.0, "preamp_db")?;
        if let Some(margin) = self.safety_margin_db {
            check(margin, 0.0, 6.0, "safety_margin_db")?;
        }
        check(self.crossfeed, 0.0, 0.3, "crossfeed")?;
        check(self.stereo_width, 0.0, 1.5, "stereo_width")?;
        for band in &self.bands {
            check(band.frequency_hz, 20.0, 20000.0, "frequency_hz")?;
            check(band.gain_db, -24.0, 24.0, "gain_db")?;
            check(band.q, 0.2, 5.0, "q")?;
            if let Some(bandwidth) = band.bandwidth_octaves {
                check(bandwidth, 0.05, 4.0, "bandwidth_octaves")?;
                for rate in [44100_u32, 48000, 96000, 192000] {
                    if band.frequency_hz < rate as f64 * 0.5 {
                        check(band.q_at(rate), 0.2, 5.0, "derived_q")?;
                    }
                }
            }
        }
        Ok(())
    }

    // Compatibility display at 48 kHz. The engine uses its negotiated sample rate.
    pub fn effective_preamp_db(&self) -> f64 {
        self.effective_preamp_at(48000)
    }
    pub fn effective_preamp_at(&self, rate: u32) -> f64 {
        if self.bypass && self.safety_margin_db.is_none() {
            return self.preamp_db;
        }
        let filters = self
            .bands
            .map(|b| crate::dsp::Coefficients::peaking(b, rate));
        let peak = crate::dsp::tone::peak_response(rate, |hz| {
            filters.iter().map(|c| c.response_db(hz, rate)).sum()
        });
        let margin = self.safety_margin_db.unwrap_or(0.5);
        self.preamp_db
            .min(if peak > 0.01 { -peak - margin } else { 0.0 })
    }

    pub fn normalize_legacy_builtin_preamp(&mut self) {
        if self.preamp_db != -6.0
            || self.safety_margin_db.is_some()
            || !PRESETS.contains(&self.name.as_str())
        {
            return;
        }
        let Ok(current) = Self::preset(&self.name) else {
            return;
        };
        let same_shape = self.crossfeed == current.crossfeed
            && self.stereo_width == current.stereo_width
            && self
                .bands
                .iter()
                .zip(current.bands.iter())
                .all(|(left, right)| {
                    left.frequency_hz == right.frequency_hz
                        && left.gain_db == right.gain_db
                        && left.q == right.q
                        && left.bandwidth_octaves == right.bandwidth_octaves
                });
        if same_shape {
            self.preamp_db = 0.0;
        }
    }

    pub fn preset(name: &str) -> Result<Self> {
        let gains = match name {
            "flat" => [0.0; 10],
            "warm" => [1.0, 2.0, 1.5, 0.5, 0.0, 0.0, -0.5, -1.0, -1.5, -1.0],
            "vocal" => [-1.0, -1.0, -0.5, 0.0, 0.5, 1.5, 2.0, 1.0, 0.0, -0.5],
            "bass" => [2.0, 3.0, 2.0, 0.5, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
            "soft" => [0.0, 0.5, 0.0, 0.0, 0.0, -0.5, -1.0, -2.0, -2.5, -2.0],
            "clarity" => [-1.0, -0.5, -0.5, -1.0, 0.0, 0.5, 1.0, 1.5, 1.0, 0.5],
            _ => bail!("Unknown preset '{name}'. Available: {}", PRESETS.join(", ")),
        };
        let mut p = Self {
            name: name.into(),
            ..Self::default()
        };
        for (band, gain) in p.bands.iter_mut().zip(gains) {
            band.gain_db = gain;
        }
        Ok(p)
    }
}

fn check(value: f64, min: f64, max: f64, field: &str) -> Result<()> {
    ensure!(
        value.is_finite() && (min..=max).contains(&value),
        "{field} must be finite and in [{min}, {max}]"
    );
    Ok(())
}
