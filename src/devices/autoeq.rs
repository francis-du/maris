//! Pinned AutoEq catalog, bundled profile lookup and explicit local profile overrides.
use crate::{control::store::Store, dsp::music::MusicProfile, tuning::preferences as listening};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub const AUTOEQ_COMMIT: &str = "7ae0f56d53074872b028649617a22bbb4232feb7";
const BUNDLED_INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/third_party/autoeq/results-index.md"
));
include!(concat!(env!("OUT_DIR"), "/autoeq_bundle.rs"));

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub source: String,
    pub form_factor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Match {
    pub device_name: String,
    pub candidate: Option<Entry>,
    pub confidence: f64,
    pub exact: bool,
    pub auto_apply: bool,
    pub reason: String,
}

fn normalize(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut space = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.split_whitespace()
        .filter(|token| {
            !matches!(
                *token,
                "bluetooth"
                    | "audio"
                    | "device"
                    | "headphone"
                    | "headphones"
                    | "stereo"
                    | "handsfree"
                    | "output"
                    | "usb"
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn generic(name: &str) -> bool {
    matches!(
        normalize(name).as_str(),
        "" | "headphones" | "headphone" | "speakers" | "speaker" | "external"
    ) || name.eq_ignore_ascii_case("Headphones")
        || name.eq_ignore_ascii_case("External Headphones")
        || name.to_ascii_lowercase().contains("usb audio")
}

fn source_from_path(path: &str) -> String {
    path.trim_start_matches("./")
        .split('/')
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

fn form_factor_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    for kind in ["over-ear", "in-ear", "earbud"] {
        if lower.contains(kind) {
            return kind.to_owned();
        }
    }
    "unknown".into()
}

pub fn parse_index(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("- [")?;
            let close = rest.find("](")?;
            let name = &rest[..close];
            let after = &rest[close + 2..];
            let end = after.find(')')?;
            let path = &after[..end];
            if !path.starts_with("./") || name.trim().is_empty() {
                return None;
            }
            Some(Entry {
                name: name.to_owned(),
                path: path.to_owned(),
                source: source_from_path(path),
                form_factor: form_factor_from_path(path),
            })
        })
        .collect()
}

pub fn catalog() -> Result<Vec<Entry>> {
    let entries = parse_index(BUNDLED_INDEX);
    ensure!(
        entries.len() > 5000,
        "Bundled AutoEq recommended-results index is incomplete"
    );
    Ok(entries)
}

/// Compatibility name for callers that previously used the local cache.
/// The catalog is now compiled from the pinned repository snapshot and never refreshed at runtime.
pub fn load_cached(_store: &Store) -> Result<Vec<Entry>> {
    catalog()
}

pub fn ensure_index(_store: &Store) -> Result<Vec<Entry>> {
    catalog()
}

fn tokens(value: &str) -> Vec<String> {
    normalize(value)
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn score(device: &str, candidate: &str) -> f64 {
    let a = normalize(device);
    let b = normalize(candidate);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let ta = tokens(device);
    let tb = tokens(candidate);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let common = ta.iter().filter(|token| tb.contains(token)).count() as f64;
    let union = (ta.len() + tb.len()) as f64 - common;
    let jaccard = if union > 0.0 { common / union } else { 0.0 };
    let contains = if b.contains(&a) || a.contains(&b) {
        0.18
    } else {
        0.0
    };
    (jaccard + contains).min(0.99)
}

pub fn match_device(device_name: &str, entries: &[Entry]) -> Match {
    if generic(device_name) {
        return Match {
            device_name: device_name.to_owned(),
            candidate: None,
            confidence: 0.0,
            exact: false,
            auto_apply: false,
            reason: "The OS endpoint name does not identify the attached headphone model".into(),
        };
    }
    let normalized = normalize(device_name);
    let mut ranked: Vec<(f64, &Entry)> = entries
        .iter()
        .map(|entry| (score(device_name, &entry.name), entry))
        .filter(|(score, _)| *score >= 0.45)
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    let Some((best_score, best)) = ranked.first().copied() else {
        return Match {
            device_name: device_name.to_owned(),
            candidate: None,
            confidence: 0.0,
            exact: false,
            auto_apply: false,
            reason: "No sufficiently similar AutoEq model was found".into(),
        };
    };
    let exact = normalize(&best.name) == normalized;
    let second = ranked.get(1).map(|(score, _)| *score).unwrap_or(0.0);
    let unique = best_score - second >= 0.08 || second < 0.5;
    let sibling_variant = entries.iter().any(|entry| {
        entry.name != best.name
            && normalize(&entry.name).starts_with(&normalized)
            && entry.name.contains('(')
            && !device_name.contains('(')
    });
    let variant_ambiguous =
        (best.name.contains('(') && !device_name.contains('(')) || sibling_variant;
    let auto_apply = exact && unique && !variant_ambiguous;
    Match {
        device_name: device_name.to_owned(),
        candidate: Some(best.clone()),
        confidence: best_score,
        exact,
        auto_apply,
        reason: if auto_apply {
            "Exact unique AutoEq model match".into()
        } else if variant_ambiguous {
            "The AutoEq model has a variant/mode that is not present in the OS device name".into()
        } else if !unique {
            "Multiple AutoEq models are similarly plausible".into()
        } else {
            "Fuzzy model match requires confirmation".into()
        },
    }
}

pub fn bundled_profiles_available() -> bool {
    AUTOEQ_PROFILES_BUNDLED && !AUTOEQ_PROFILE_PACK.is_empty()
}

fn pack_u32(bytes: &[u8], cursor: &mut usize) -> Result<usize> {
    ensure!(
        *cursor + 4 <= bytes.len(),
        "Bundled AutoEq profile pack is truncated"
    );
    let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into()?);
    *cursor += 4;
    Ok(value as usize)
}

fn bundled_profile_text(entry: &Entry) -> Result<&'static str> {
    ensure!(
        bundled_profiles_available(),
        "This development build does not include the AutoEq profile pack"
    );
    ensure!(
        AUTOEQ_PROFILE_SOURCE_COMMIT == AUTOEQ_COMMIT,
        "Bundled AutoEq profile pack uses a different source commit"
    );
    ensure!(
        AUTOEQ_PROFILE_PACK_SHA256.len() == 64,
        "Bundled AutoEq profile pack has no verified digest"
    );
    const HEADER: &[u8] = b"MARIS_AUTOEQ_V1\0";
    ensure!(
        AUTOEQ_PROFILE_PACK.starts_with(HEADER),
        "Bundled AutoEq profile pack header is invalid"
    );
    let mut cursor = HEADER.len();
    let count = pack_u32(AUTOEQ_PROFILE_PACK, &mut cursor)?;
    ensure!(count > 5_000, "Bundled AutoEq profile pack is incomplete");
    for _ in 0..count {
        let path_len = pack_u32(AUTOEQ_PROFILE_PACK, &mut cursor)?;
        let text_len = pack_u32(AUTOEQ_PROFILE_PACK, &mut cursor)?;
        ensure!(
            path_len > 2 && path_len < 4_096 && text_len > 16 && text_len < 64 * 1024,
            "Bundled AutoEq profile record is invalid"
        );
        let end = cursor
            .checked_add(path_len)
            .and_then(|value| value.checked_add(text_len))
            .context("Bundled AutoEq profile record overflow")?;
        ensure!(
            end <= AUTOEQ_PROFILE_PACK.len(),
            "Bundled AutoEq profile pack is truncated"
        );
        let path = std::str::from_utf8(&AUTOEQ_PROFILE_PACK[cursor..cursor + path_len])?;
        cursor += path_len;
        let text = std::str::from_utf8(&AUTOEQ_PROFILE_PACK[cursor..cursor + text_len])?;
        cursor += text_len;
        if path == entry.path {
            return Ok(text);
        }
    }
    anyhow::bail!("No bundled AutoEq profile is available for {}", entry.name)
}

pub fn bundled_profile(entry: &Entry) -> Result<MusicProfile> {
    let source = format!(
        "AutoEq@{} / {} / {}",
        &AUTOEQ_COMMIT[..12],
        entry.source,
        entry.name
    );
    listening::import_peq(bundled_profile_text(entry)?, &source)
}

fn cached_profile_path(store: &Store, entry: &Entry) -> PathBuf {
    let file_name = format!(
        "{}.json",
        normalize(&entry.name)
            .replace(' ', "-")
            .chars()
            .take(96)
            .collect::<String>()
    );
    store.directory.join("autoeq-profiles").join(file_name)
}

pub fn cache_profile(store: &Store, entry: &Entry, profile: &MusicProfile) -> Result<PathBuf> {
    let path = cached_profile_path(store, entry);
    let directory = path.parent().context("Invalid AutoEq cache path")?;
    fs::create_dir_all(directory)?;
    let payload = serde_json::json!({
        "commit": AUTOEQ_COMMIT,
        "entry": entry,
        "profile": profile
    });
    fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(path)
}

pub fn load_cached_profile(store: &Store, entry: &Entry) -> Result<MusicProfile> {
    let path = cached_profile_path(store, entry);
    let value: serde_json::Value = crate::control::store::read_json(&path)
        .with_context(|| format!("AutoEq profile is not cached for {}", entry.name))?;
    ensure!(
        value["commit"].as_str() == Some(AUTOEQ_COMMIT),
        "Cached AutoEq profile uses a different source commit"
    );
    let cached_entry: Entry = serde_json::from_value(value["entry"].clone())?;
    ensure!(
        cached_entry.name == entry.name && cached_entry.path == entry.path,
        "Cached AutoEq profile does not match the requested model"
    );
    let profile: MusicProfile = serde_json::from_value(value["profile"].clone())?;
    profile.validate()?;
    Ok(profile)
}

pub fn profile_for(store: &Store, entry: &Entry) -> Result<MusicProfile> {
    load_cached_profile(store, entry).or_else(|cache_error| {
        bundled_profile(entry).with_context(|| {
            format!(
                "No valid local AutoEq profile is available for {} (cache: {cache_error:#})",
                entry.name
            )
        })
    })
}

pub fn resolve(device_name: &str, store: &Store) -> Result<Match> {
    let entries = ensure_index(store)?;
    Ok(match_device(device_name, &entries))
}

pub fn apply_entry(
    store: &Store,
    device_name: &str,
    entry: &Entry,
) -> Result<crate::tuning::preferences::Library> {
    let profile = profile_for(store, entry)?;
    let correction_source = profile.correction_source.clone();
    let library = crate::tuning::preferences::edit(store, None, Some(device_name), |current| {
        let preference = (
            current.bass_db,
            current.presence_db,
            current.air_db,
            current.softness,
            current.intensity,
            current.width,
            current.balance,
            current.compressor,
            current.adaptive,
            current.bass_assist,
        );
        current.correction = profile.correction.clone();
        current.correction_preamp_db = profile.correction_preamp_db;
        current.correction_source = profile.correction_source.clone();
        current.bass_db = preference.0;
        current.presence_db = preference.1;
        current.air_db = preference.2;
        current.softness = preference.3;
        current.intensity = preference.4;
        current.width = preference.5;
        current.balance = preference.6;
        current.compressor = preference.7;
        current.adaptive = preference.8;
        current.bass_assist = preference.9;
        Ok(())
    })?;
    let _ = crate::devices::capability::remember_applied_correction(
        store,
        device_name,
        entry,
        correction_source.as_deref(),
    );
    Ok(library)
}

pub fn bind_entry(
    store: &Store,
    device_name: &str,
    entry: &Entry,
) -> Result<crate::tuning::preferences::Library> {
    let matched = Match {
        device_name: device_name.to_owned(),
        candidate: Some(entry.clone()),
        confidence: 1.0,
        exact: normalize(device_name) == normalize(&entry.name),
        auto_apply: false,
        reason: "Manually confirmed endpoint-to-model binding".into(),
    };
    crate::devices::capability::remember_match(store, device_name, &matched, true)?;
    apply_entry(store, device_name, entry)
}
