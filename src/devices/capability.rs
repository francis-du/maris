//! Device capability and correction binding state.
use crate::{control::store::Store, devices::autoeq};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::OpenOptions};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Capability {
    pub model: Option<String>,
    pub correction_source: Option<String>,
    pub autoeq_path: Option<String>,
    pub match_confidence: f64,
    pub match_confirmed: bool,
    pub match_reason: Option<String>,
    pub bass_floor_hz: Option<f64>,
    pub treble_ceiling_hz: Option<f64>,
    pub max_preference_boost_db: f64,
    pub virtual_bass_allowed: bool,
    pub device_class: String,
}
impl Default for Capability {
    fn default() -> Self {
        Self {
            model: None,
            correction_source: None,
            autoeq_path: None,
            match_confidence: 0.0,
            match_confirmed: false,
            match_reason: None,
            bass_floor_hz: None,
            treble_ceiling_hz: None,
            max_preference_boost_db: 3.0,
            virtual_bass_allowed: false,
            device_class: "unknown".into(),
        }
    }
}
impl Capability {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.match_confidence.is_finite() && (0.0..=1.0).contains(&self.match_confidence),
            "match_confidence must be in [0, 1]"
        );
        if let Some(reason) = &self.match_reason {
            ensure!(
                !reason.is_empty() && reason.len() <= 512 && !reason.chars().any(char::is_control),
                "Invalid AutoEq match reason"
            );
        }
        if let Some(hz) = self.bass_floor_hz {
            ensure!(
                hz.is_finite() && (10.0..=500.0).contains(&hz),
                "Invalid bass floor"
            );
        }
        if let Some(hz) = self.treble_ceiling_hz {
            ensure!(
                hz.is_finite() && (1000.0..=30000.0).contains(&hz),
                "Invalid treble ceiling"
            );
        }
        ensure!(
            self.max_preference_boost_db.is_finite()
                && (0.0..=6.0).contains(&self.max_preference_boost_db),
            "max_preference_boost_db must be in [0, 6]"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Library {
    pub revision: u64,
    pub devices: BTreeMap<String, Capability>,
}
impl Library {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.devices.len() <= 64,
            "Too many device capability records"
        );
        for (name, capability) in &self.devices {
            ensure!(
                !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control),
                "Invalid device name"
            );
            capability.validate()?;
        }
        Ok(())
    }
}

pub fn load(store: &Store) -> Result<Library> {
    let path = store.directory.join("device-capabilities.json");
    if !path.exists() {
        return Ok(Library::default());
    }
    let library: Library = crate::control::store::read_json(&path)?;
    library.validate()?;
    Ok(library)
}

fn classify(name: &str) -> Capability {
    let lower = name.to_ascii_lowercase();
    if lower.contains("macbook") && lower.contains("speaker") {
        Capability {
            virtual_bass_allowed: true,
            max_preference_boost_db: 1.5,
            device_class: "built_in_compact_speaker".into(),
            ..Capability::default()
        }
    } else if lower.contains("speaker") {
        Capability {
            virtual_bass_allowed: true,
            max_preference_boost_db: 2.0,
            device_class: "speaker".into(),
            ..Capability::default()
        }
    } else if lower.contains("airpods")
        || lower.contains("buds")
        || lower.contains("headphone")
        || lower.contains("wh-")
        || lower.contains("wf-")
    {
        Capability {
            virtual_bass_allowed: false,
            max_preference_boost_db: 3.0,
            device_class: "headphone".into(),
            ..Capability::default()
        }
    } else {
        Capability::default()
    }
}

pub fn effective(store: &Store, device_name: &str) -> Result<Capability> {
    let library = load(store)?;
    Ok(library
        .devices
        .get(device_name)
        .cloned()
        .unwrap_or_else(|| classify(device_name)))
}

fn write(
    store: &Store,
    device_name: &str,
    update: impl FnOnce(&mut Capability) -> Result<()>,
) -> Result<Library> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("device-capabilities.lock"))?;
    lock.lock_exclusive()?;
    let mut library = load(store)?;
    let capability = library
        .devices
        .entry(device_name.to_owned())
        .or_insert_with(|| classify(device_name));
    update(capability)?;
    capability.validate()?;
    library.revision = library
        .revision
        .checked_add(1)
        .context("Device capability revision overflow")?;
    library.validate()?;
    store.write_json("device-capabilities.json", &library)?;
    Ok(library)
}

pub fn remember_match(
    store: &Store,
    device_name: &str,
    matched: &autoeq::Match,
    confirmed: bool,
) -> Result<Library> {
    write(store, device_name, |capability| {
        capability.match_confidence = matched.confidence;
        capability.match_confirmed = confirmed || matched.auto_apply;
        capability.match_reason = Some(matched.reason.clone());
        if let Some(entry) = &matched.candidate {
            capability.model = Some(entry.name.clone());
            capability.autoeq_path = Some(entry.path.clone());
            capability.device_class = if entry.form_factor == "unknown" {
                capability.device_class.clone()
            } else {
                "headphone".into()
            };
            capability.virtual_bass_allowed = false;
        }
        Ok(())
    })
}

pub fn remember_applied_correction(
    store: &Store,
    device_name: &str,
    entry: &autoeq::Entry,
    correction_source: Option<&str>,
) -> Result<Library> {
    write(store, device_name, |capability| {
        capability.model = Some(entry.name.clone());
        capability.autoeq_path = Some(entry.path.clone());
        capability.correction_source = correction_source.map(str::to_owned);
        capability.match_confirmed = true;
        capability.match_reason = Some("Correction profile applied".into());
        if entry.form_factor != "unknown" {
            capability.device_class = "headphone".into();
            capability.virtual_bass_allowed = false;
        }
        Ok(())
    })
}

pub fn apply_constraints(
    profile: &crate::dsp::music::MusicProfile,
    capability: &Capability,
) -> crate::dsp::music::MusicProfile {
    let mut effective = profile.clone();
    let cap = capability.max_preference_boost_db;
    effective.bass_db = effective.bass_db.min(cap);
    effective.presence_db = effective.presence_db.min(cap.min(3.0));
    effective.air_db = effective.air_db.min(cap.min(3.0));
    if !capability.virtual_bass_allowed {
        effective.bass_assist.enabled = false;
    }
    effective
}

pub fn inspect(store: &Store, device_name: &str) -> Result<serde_json::Value> {
    let capability = effective(store, device_name)?;
    Ok(serde_json::json!({
        "device": device_name,
        "capability": capability,
        "physical_limits": {
            "bass_floor_hz": capability.bass_floor_hz,
            "treble_ceiling_hz": capability.treble_ceiling_hz,
            "known": capability.bass_floor_hz.is_some() || capability.treble_ceiling_hz.is_some(),
            "note": "Unknown limits are never inferred from the song spectrum or from a generic OS endpoint name."
        }
    }))
}
