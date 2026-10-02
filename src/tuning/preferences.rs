//! Versioned per-output listening profiles. Unknown devices never inherit another device's correction.
use crate::{
    control::store::{self, Store, MAX_JSON_BYTES},
    dsp::music::MusicProfile,
    dsp::tone::{Filter, Kind},
};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Library {
    pub revision: u64,
    pub default: MusicProfile,
    pub devices: BTreeMap<String, MusicProfile>,
}
impl Library {
    pub fn effective(&self, device: &str) -> &MusicProfile {
        self.devices.get(device).unwrap_or(&self.default)
    }
    fn validate(&self) -> Result<()> {
        self.default.validate()?;
        ensure!(
            self.devices.len() <= 24,
            "At most 24 device listening profiles are supported"
        );
        for (device, p) in &self.devices {
            ensure!(
                !device.is_empty() && device.len() <= 256 && !device.chars().any(char::is_control),
                "Invalid output device name"
            );
            p.validate()?;
        }
        Ok(())
    }
}
pub fn load(store: &Store) -> Result<Library> {
    let path = store.directory.join("listening.json");
    if !path.exists() {
        return Ok(Library::default());
    }
    let library: Library = store::read_json(&path)?;
    library.validate()?;
    Ok(library)
}
pub fn edit(
    store: &Store,
    expected: Option<u64>,
    device: Option<&str>,
    change: impl FnOnce(&mut MusicProfile) -> Result<()>,
) -> Result<Library> {
    edit_inner(store, expected, device, EditKind::Persistent, change)
}

/// A control adjustment only records an actual effective-parameter change. Explicit
/// save/copy operations may still use edit() to materialize a fallback device profile.
pub fn edit_if_changed(
    store: &Store,
    expected: Option<u64>,
    device: Option<&str>,
    change: impl FnOnce(&mut MusicProfile) -> Result<()>,
) -> Result<Library> {
    edit_inner(store, expected, device, EditKind::Adjustment, change)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Persistent,
    Adjustment,
    Comparison,
}

/// A/B is a revisioned parameter update, not a new tonal edit in the undo history.
pub fn compare(
    store: &Store,
    expected: Option<u64>,
    device: Option<&str>,
    reference: bool,
) -> Result<Library> {
    edit_inner(store, expected, device, EditKind::Comparison, |profile| {
        profile.reference = reference;
        Ok(())
    })
}

fn edit_inner(
    store: &Store,
    expected: Option<u64>,
    device: Option<&str>,
    kind: EditKind,
    change: impl FnOnce(&mut MusicProfile) -> Result<()>,
) -> Result<Library> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("listening.lock"))?;
    lock.lock_exclusive()?;
    let mut library = load(store)?;
    if let Some(expected) = expected {
        ensure!(
            library.revision == expected,
            "Listening revision conflict: expected {expected}, current {}",
            library.revision
        );
    }
    let prior = library.clone();
    let target = if let Some(name) = device {
        library
            .devices
            .entry(name.to_owned())
            .or_insert_with(|| prior.default.clone())
    } else {
        &mut library.default
    };
    change(target)?;
    library.validate()?;
    if library.default == prior.default && library.devices == prior.devices {
        return Ok(prior);
    }
    if kind != EditKind::Persistent {
        let before = device.map_or(&prior.default, |key| prior.effective(key));
        let after = device.map_or(&library.default, |key| library.effective(key));
        if before == after {
            return Ok(prior);
        }
    }
    library.revision = library
        .revision
        .checked_add(1)
        .context("Listening revision overflow")?;
    ensure!(
        (serde_json::to_vec_pretty(&library)?.len() as u64) < MAX_JSON_BYTES,
        "Listening profiles exceed the bounded storage size"
    );
    if kind != EditKind::Comparison {
        store.write_json("listening-previous.json", &prior)?;
    }
    store.write_json("listening.json", &library)?;
    Ok(library)
}
pub fn undo(store: &Store, expected: Option<u64>) -> Result<Library> {
    undo_inner(store, expected, None)
}

fn same_tuning(left: &MusicProfile, right: &MusicProfile) -> bool {
    let mut normalized = left.clone();
    normalized.reference = right.reference;
    normalized == *right
}

fn scoped_history(current: &Library, previous: &Library, device: &str) -> bool {
    // Comparison is not a tonal edit, including when it materializes an unrelated
    // endpoint from the fallback. Only unrelated tuning changes invalidate undo.
    same_tuning(&current.default, &previous.default)
        && current
            .devices
            .keys()
            .chain(previous.devices.keys())
            .all(|key| {
                key == device || same_tuning(current.effective(key), previous.effective(key))
            })
        && !same_tuning(previous.effective(device), current.effective(device))
}

/// Never offer a current-device undo that would restore another endpoint's settings.
pub fn can_undo_device(store: &Store, current: &Library, device: &str) -> Result<bool> {
    let path = store.directory.join("listening-previous.json");
    if !path.exists() {
        return Ok(false);
    }
    let previous: Library = store::read_json(&path)?;
    previous.validate()?;
    Ok(scoped_history(current, &previous, device))
}

pub fn undo_device(store: &Store, expected: u64, device: &str) -> Result<Library> {
    undo_inner(store, Some(expected), Some(device))
}

fn undo_inner(store: &Store, expected: Option<u64>, device: Option<&str>) -> Result<Library> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("listening.lock"))?;
    lock.lock_exclusive()?;
    let current = load(store)?;
    if let Some(expected) = expected {
        ensure!(
            current.revision == expected,
            "Listening revision conflict: expected {expected}, current {}",
            current.revision
        );
    }
    let previous_path = store.directory.join("listening-previous.json");
    ensure!(
        previous_path.exists(),
        "No previous listening profile to restore"
    );
    let mut previous: Library = store::read_json(&previous_path)?;
    previous.validate()?;
    if let Some(device) = device {
        ensure!(
            scoped_history(&current, &previous, device),
            "No undo for the current output"
        );
        // Restore only the requested endpoint. Retain all other endpoints and the
        // fallback exactly, including A/B changes made after this tonal snapshot.
        let target = previous.devices.get(device).cloned();
        previous = current.clone();
        if let Some(target) = target {
            previous.devices.insert(device.to_owned(), target);
        } else {
            previous.devices.remove(device);
        }
    }
    previous.revision = current
        .revision
        .checked_add(1)
        .context("Listening revision overflow")?;
    store.write_json("listening-previous.json", &current)?;
    store.write_json("listening.json", &previous)?;
    Ok(previous)
}

pub fn preset(
    store: &Store,
    expected: Option<u64>,
    device: Option<&str>,
    name: &str,
) -> Result<Library> {
    MusicProfile::preset(name.strip_prefix("scene:").unwrap_or(name))?;
    let capability = device
        .map(|key| crate::devices::capability::effective(store, key))
        .transpose()?
        .unwrap_or_default();
    edit_if_changed(store, expected, device, |profile| {
        *profile = crate::presets::scenes::prepare(profile, &capability, name)?.profile;
        Ok(())
    })
}

pub fn current_device(store: &Store) -> Result<String> {
    let runtime = crate::audio::runtime_status(store);
    ensure!(
        runtime["active"] == true,
        "Start Maris before saving the current output profile"
    );
    Ok(runtime["profile_key"]
        .as_str()
        .or_else(|| runtime["output"].as_str())
        .context("Missing output device")?
        .to_owned())
}

/// Import a strict subset of Equalizer APO/AutoEq PEQ, rejecting unknown enabled operations.
pub fn import_peq(text: &str, source: &str) -> Result<MusicProfile> {
    ensure!(
        text.len() as u64 <= MAX_JSON_BYTES,
        "PEQ input is too large"
    );
    let mut profile = MusicProfile {
        correction_source: Some(source.to_owned()),
        ..MusicProfile::default()
    };
    let mut numbers = BTreeSet::new();
    let mut preamp_seen = false;
    for original in text.lines() {
        let line = original.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<_> = line.split_whitespace().collect();
        if tokens.first() == Some(&"Preamp:") {
            ensure!(
                !preamp_seen && tokens.len() == 3 && tokens[2] == "dB",
                "Invalid or duplicate preamp"
            );
            profile.correction_preamp_db = tokens[1].parse()?;
            preamp_seen = true;
            continue;
        }
        ensure!(
            tokens.first() == Some(&"Filter") && tokens.len() >= 4,
            "Unsupported PEQ line: {line}"
        );
        let number: usize = tokens[1]
            .strip_suffix(':')
            .context("Invalid filter index")?
            .parse()?;
        ensure!(
            number > 0 && numbers.insert(number),
            "Duplicate or invalid filter index"
        );
        if tokens[2] == "OFF" {
            continue;
        }
        ensure!(
            tokens[2] == "ON"
                && tokens.len() == 12
                && tokens[4] == "Fc"
                && tokens[6] == "Hz"
                && tokens[7] == "Gain"
                && tokens[9] == "dB"
                && tokens[10] == "Q",
            "Unsupported PEQ syntax: {line}"
        );
        let kind = match tokens[3] {
            "PK" => Kind::Peak,
            "LSC" | "LS" => Kind::LowShelf,
            "HSC" | "HS" => Kind::HighShelf,
            _ => anyhow::bail!("Unsupported PEQ filter kind: {}", tokens[3]),
        };
        profile.correction.push(Filter {
            kind,
            frequency_hz: tokens[5].parse()?,
            gain_db: tokens[8].parse()?,
            q: tokens[11].parse()?,
        });
    }
    ensure!(
        !profile.correction.is_empty(),
        "PEQ contains no enabled filters"
    );
    profile.validate()?;
    Ok(profile)
}
