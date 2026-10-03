//! Unified built-in preset catalog. eqMac data is generated from a vendored, fixed-commit source snapshot.
pub mod scenes;

use crate::{
    dsp::profile::{Band, Profile, PRESETS as MARIS_PRESETS},
    dsp::tone,
};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::{error::Error, f64::consts::TAU, fmt};

pub const EQMAC_FREQUENCIES_HZ: [f64; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
pub const EQMAC_BANDWIDTH_OCTAVES: f64 = 0.5;
pub const SAFETY_MARGIN_DB: f64 = 0.5;

#[derive(Debug)]
pub struct PresetNotFound(pub String);
impl fmt::Display for PresetNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown preset '{}'", self.0)
    }
}
impl Error for PresetNotFound {}

#[derive(Clone, Copy, Debug)]
pub struct EqMacRawPreset {
    pub id: &'static str,
    pub original_name: &'static str,
    pub gains_db: [f64; 10],
}

include!(concat!(env!("OUT_DIR"), "/eqmac_presets.rs"));

#[derive(Clone, Debug, Serialize)]
pub struct PresetSummary {
    pub id: String,
    pub category: &'static str,
    pub name: String,
    pub source: &'static str,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresetDetails {
    pub id: String,
    pub category: &'static str,
    pub name: String,
    pub source: &'static str,
    pub source_commit: Option<&'static str>,
    pub source_global_gain_db: f64,
    pub source_frequencies_hz: [f64; 10],
    pub source_gains_db: [f64; 10],
    pub source_bandwidth_octaves: Option<f64>,
    pub converted_q: [f64; 10],
    pub safety_margin_db: f64,
    pub safe_preamp_db: f64,
    pub sample_rate: u32,
}

fn validate_rate(rate: u32) -> Result<()> {
    ensure!(
        (44100..=192000).contains(&rate),
        "Preset conversion supports 44100..192000 Hz"
    );
    Ok(())
}

pub fn bandwidth_to_q(bandwidth_octaves: f64, frequency_hz: f64, rate: u32) -> Result<f64> {
    validate_rate(rate)?;
    ensure!(
        bandwidth_octaves.is_finite() && (0.05..=4.0).contains(&bandwidth_octaves),
        "Bandwidth must be 0.05..4 octaves"
    );
    ensure!(
        frequency_hz.is_finite()
            && (20.0..=20000.0).contains(&frequency_hz)
            && frequency_hz < rate as f64 * 0.5,
        "Preset frequency must be inside the supported Nyquist range"
    );
    let w0 = TAU * frequency_hz / rate as f64;
    let warping = if w0.sin().abs() > 1e-12 {
        w0 / w0.sin()
    } else {
        1.0
    };
    let q = 1.0 / (2.0 * (std::f64::consts::LN_2 * 0.5 * bandwidth_octaves * warping).sinh());
    ensure!(
        q.is_finite() && (0.2..=5.0).contains(&q),
        "Derived Q is unsupported"
    );
    Ok(q)
}

fn eqmac_profile(raw: EqMacRawPreset, rate: u32) -> Result<Profile> {
    validate_rate(rate)?;
    let bands = std::array::from_fn(|index| {
        let frequency_hz = EQMAC_FREQUENCIES_HZ[index];
        Band {
            frequency_hz,
            gain_db: raw.gains_db[index],
            q: bandwidth_to_q(EQMAC_BANDWIDTH_OCTAVES, frequency_hz, rate)
                .expect("validated fixed eqMac band"),
            bandwidth_octaves: Some(EQMAC_BANDWIDTH_OCTAVES),
        }
    });
    let profile = Profile {
        name: raw.id.to_owned(),
        preamp_db: 0.0,
        safety_margin_db: Some(SAFETY_MARGIN_DB),
        bands,
        crossfeed: 0.0,
        stereo_width: 1.0,
        bypass: false,
    };
    profile.validate()?;
    Ok(profile)
}

pub fn tone_curve_matches(current: &Profile, id: &str, rate: u32) -> bool {
    profile(id, rate).is_ok_and(|target| {
        current.name == target.name
            && current.preamp_db == target.preamp_db
            && current.safety_margin_db == target.safety_margin_db
            && current.bands == target.bands
    })
}

pub fn apply_tone_curve(current: &Profile, id: &str, rate: u32) -> Result<Profile> {
    let mut next = profile(id, rate)?;
    // A tone-curve preset owns its curve/headroom metadata, not independent
    // playback/spatial policy. Preserve controls that the user may have set
    // through Settings, Menu Bar, CLI or MCP.
    next.crossfeed = current.crossfeed;
    next.stereo_width = current.stereo_width;
    next.bypass = current.bypass;
    next.validate()?;
    Ok(next)
}

pub fn profile(id: &str, rate: u32) -> Result<Profile> {
    if MARIS_PRESETS.contains(&id) {
        return Profile::preset(id);
    }
    if id == "eqmac:flat" {
        return Profile::preset("flat");
    }
    let raw = EQMAC_RAW_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id && preset.original_name != "Flat")
        .ok_or_else(|| PresetNotFound(id.to_owned()))?;
    eqmac_profile(raw, rate)
}

fn maris_description(id: &str) -> &'static str {
    match id {
        "flat" => "Maris neutral unit-response curve; also reuses eqMac Flat semantics.",
        "warm" => "Legacy Maris warm tonal curve.",
        "vocal" => "Legacy Maris vocal-forward tonal curve.",
        "bass" => "Legacy Maris bass-emphasis tonal curve.",
        "soft" => "Legacy Maris softer high-frequency tonal curve.",
        "clarity" => "Legacy Maris clarity tonal curve.",
        _ => "Legacy Maris built-in preset.",
    }
}

pub fn list() -> Vec<PresetSummary> {
    let mut presets = Vec::with_capacity(MARIS_PRESETS.len() + EQMAC_RAW_PRESETS.len() - 1);
    presets.extend(MARIS_PRESETS.into_iter().map(|id| PresetSummary {
        id: id.to_owned(),
        category: "maris",
        name: id.to_owned(),
        source: "Maris",
        description: maris_description(id).to_owned(),
    }));
    presets.extend(
        EQMAC_RAW_PRESETS
            .iter()
            .filter(|preset| preset.original_name != "Flat")
            .map(|preset| PresetSummary {
                id: preset.id.to_owned(),
                category: "eqmac",
                name: preset.original_name.to_owned(),
                source: "eqMac open-source preset set",
                description: format!(
                    "eqMac ten-band preset '{}' converted from 0.5-octave bandwidth at runtime.",
                    preset.original_name
                ),
            }),
    );
    presets
}

/// Customer picker: useful listening scenes first; original EQ IDs remain unchanged.
pub const LISTENING_PRESETS: [&str; 6] = ["natural", "warm", "vocal", "detail", "soft", "night"];

pub fn console_catalog() -> Vec<PresetSummary> {
    let mut catalog: Vec<_> = LISTENING_PRESETS
        .into_iter()
        .map(|id| PresetSummary {
            id: format!("listening:{id}"),
            category: "listening",
            name: id.to_owned(),
            source: "Maris",
            description: "Device listening preset; measured correction stays unchanged.".into(),
        })
        .collect();
    catalog.extend(
        crate::presets::scenes::SCENES
            .iter()
            .map(|scene| PresetSummary {
                id: format!("scene:{}", scene.id),
                category: "scene",
                name: scene.name.into(),
                source: "Maris",
                description: scene.description.into(),
            })
            .collect::<Vec<_>>(),
    );
    catalog.extend(list());
    catalog
}

pub fn ids() -> Vec<String> {
    list().into_iter().map(|preset| preset.id).collect()
}

pub fn show(id: &str, rate: u32) -> Result<PresetDetails> {
    validate_rate(rate)?;
    if MARIS_PRESETS.contains(&id) {
        let profile = Profile::preset(id)?;
        return Ok(PresetDetails {
            id: id.to_owned(),
            category: "maris",
            name: id.to_owned(),
            source: "Maris",
            source_commit: None,
            source_global_gain_db: profile.preamp_db,
            source_frequencies_hz: profile.bands.map(|band| band.frequency_hz),
            source_gains_db: profile.bands.map(|band| band.gain_db),
            source_bandwidth_octaves: None,
            converted_q: profile.bands.map(|band| band.q_at(rate)),
            safety_margin_db: SAFETY_MARGIN_DB,
            safe_preamp_db: profile.effective_preamp_at(rate),
            sample_rate: rate,
        });
    }
    if id == "eqmac:flat" {
        let profile = Profile::preset("flat")?;
        return Ok(PresetDetails {
            id: "flat".to_owned(),
            category: "maris",
            name: "flat".to_owned(),
            source: "Maris; equivalent unit response to eqMac Flat",
            source_commit: Some(EQMAC_SOURCE_COMMIT),
            source_global_gain_db: 0.0,
            source_frequencies_hz: EQMAC_FREQUENCIES_HZ,
            source_gains_db: [0.0; 10],
            source_bandwidth_octaves: Some(EQMAC_BANDWIDTH_OCTAVES),
            converted_q: profile.bands.map(|band| band.q_at(rate)),
            safety_margin_db: SAFETY_MARGIN_DB,
            safe_preamp_db: 0.0,
            sample_rate: rate,
        });
    }
    let raw = EQMAC_RAW_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id && preset.original_name != "Flat")
        .ok_or_else(|| PresetNotFound(id.to_owned()))?;
    let profile = eqmac_profile(raw, rate)?;
    Ok(PresetDetails {
        id: raw.id.to_owned(),
        category: "eqmac",
        name: raw.original_name.to_owned(),
        source: "eqMac open-source preset set",
        source_commit: Some(EQMAC_SOURCE_COMMIT),
        source_global_gain_db: 0.0,
        source_frequencies_hz: EQMAC_FREQUENCIES_HZ,
        source_gains_db: raw.gains_db,
        source_bandwidth_octaves: Some(EQMAC_BANDWIDTH_OCTAVES),
        converted_q: profile.bands.map(|band| band.q_at(rate)),
        safety_margin_db: SAFETY_MARGIN_DB,
        safe_preamp_db: profile.effective_preamp_at(rate),
        sample_rate: rate,
    })
}

pub fn response_peak_db(profile: &Profile, rate: u32) -> Result<f64> {
    validate_rate(rate)?;
    profile.validate()?;
    ensure!(
        profile
            .bands
            .iter()
            .all(|band| band.frequency_hz < rate as f64 * 0.5),
        "Preset band exceeds Nyquist"
    );
    let coefficients = profile
        .bands
        .map(|band| crate::dsp::Coefficients::peaking(band, rate));
    Ok(tone::peak_response(rate, |hz| {
        coefficients
            .iter()
            .map(|coefficient| coefficient.response_db(hz, rate))
            .sum()
    }))
}

pub fn raw_eqmac() -> &'static [EqMacRawPreset] {
    EQMAC_RAW_PRESETS
}

pub fn eqmac_by_original_name(name: &str) -> Result<EqMacRawPreset> {
    EQMAC_RAW_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.original_name == name)
        .ok_or_else(|| PresetNotFound(name.to_owned()).into())
}

pub fn assert_source_contract() -> Result<()> {
    ensure!(
        EQMAC_RAW_PRESETS.len() == 22,
        "eqMac source preset count changed"
    );
    ensure!(
        EQMAC_RAW_PRESETS
            .iter()
            .all(|preset| preset.gains_db.iter().all(|gain| gain.is_finite())),
        "eqMac source contains non-finite gains"
    );
    let flat = eqmac_by_original_name("Flat")?;
    ensure!(flat.gains_db == [0.0; 10], "eqMac Flat is not flat");
    let acoustic = eqmac_by_original_name("Acoustic")?;
    ensure!(
        acoustic
            .gains_db
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            == 18.22,
        "eqMac Acoustic source maximum changed"
    );
    Ok(())
}
