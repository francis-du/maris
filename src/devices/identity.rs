//! Stable device identity bindings. Display names remain human-readable aliases; stable IDs are platform evidence.

use crate::control::store::Store;
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::OpenOptions};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub platform: String,
    pub stable_id: Option<String>,
    pub model_id: Option<String>,
    pub display_name: String,
    pub source: String,
}

impl Identity {
    pub fn display_name(name: &str) -> Self {
        Self {
            platform: std::env::consts::OS.to_owned(),
            stable_id: None,
            model_id: None,
            display_name: name.to_owned(),
            source: "display_name".into(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        valid_string(&self.platform, 32, "platform")?;
        valid_string(&self.display_name, 256, "display_name")?;
        valid_string(&self.source, 64, "source")?;
        if let Some(value) = &self.stable_id {
            valid_string(value, 512, "stable_id")?;
        }
        if let Some(value) = &self.model_id {
            valid_string(value, 512, "model_id")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub stable_id: String,
    pub model_id: Option<String>,
    pub display_name: String,
    pub profile_key: String,
    pub updated_at_ms: u64,
}

impl Binding {
    fn validate(&self) -> Result<()> {
        valid_string(&self.stable_id, 512, "stable_id")?;
        valid_string(&self.display_name, 256, "display_name")?;
        valid_string(&self.profile_key, 256, "profile_key")?;
        if let Some(value) = &self.model_id {
            valid_string(value, 512, "model_id")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Library {
    pub revision: u64,
    pub bindings: BTreeMap<String, Binding>,
}

impl Library {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.bindings.len() <= 128,
            "At most 128 stable device bindings are supported"
        );
        for (stable_id, binding) in &self.bindings {
            binding.validate()?;
            ensure!(
                stable_id == &binding.stable_id,
                "Device binding key does not match its stable ID"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub identity: Identity,
    pub profile_key: String,
    pub binding_source: String,
    pub binding_revision: u64,
}

fn stable_suffix(stable_id: &str) -> String {
    let mut chars: Vec<char> = stable_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    if chars.is_empty() {
        chars = stable_id
            .as_bytes()
            .iter()
            .take(6)
            .flat_map(|byte| format!("{byte:02x}").chars().collect::<Vec<_>>())
            .collect();
    }
    let start = chars.len().saturating_sub(12);
    chars[start..].iter().collect()
}

fn unique_profile_key(library: &Library, stable_id: &str, display_name: &str) -> Result<String> {
    if !library
        .bindings
        .values()
        .any(|binding| binding.stable_id != stable_id && binding.profile_key == display_name)
    {
        return Ok(display_name.to_owned());
    }
    let suffix = format!(" @{}", stable_suffix(stable_id));
    let budget = 256_usize.saturating_sub(suffix.len());
    let mut base = String::new();
    for ch in display_name.chars() {
        if base.len() + ch.len_utf8() > budget {
            break;
        }
        base.push(ch);
    }
    let candidate = format!("{base}{suffix}");
    if !library
        .bindings
        .values()
        .any(|binding| binding.stable_id != stable_id && binding.profile_key == candidate)
    {
        return Ok(candidate);
    }
    for index in 2..=128 {
        let numbered_suffix = format!("{suffix}#{index}");
        let budget = 256_usize.saturating_sub(numbered_suffix.len());
        let mut base = String::new();
        for ch in display_name.chars() {
            if base.len() + ch.len_utf8() > budget {
                break;
            }
            base.push(ch);
        }
        let candidate = format!("{base}{numbered_suffix}");
        if !library
            .bindings
            .values()
            .any(|binding| binding.stable_id != stable_id && binding.profile_key == candidate)
        {
            return Ok(candidate);
        }
    }
    anyhow::bail!("Unable to allocate a unique device profile key")
}

fn valid_string(value: &str, max: usize, field: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control),
        "Invalid {field}"
    );
    Ok(())
}

pub fn load(store: &Store) -> Result<Library> {
    let path = store.directory.join("device-bindings.json");
    if !path.exists() {
        return Ok(Library::default());
    }
    let library: Library = crate::control::store::read_json(&path)?;
    library.validate()?;
    Ok(library)
}

fn write(store: &Store, update: impl FnOnce(&mut Library) -> Result<bool>) -> Result<Library> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("device-bindings.lock"))?;
    lock.lock_exclusive()?;
    let mut library = load(store)?;
    if update(&mut library)? {
        library.revision = library
            .revision
            .checked_add(1)
            .context("Device binding revision overflow")?;
        library.validate()?;
        store.write_json("device-bindings.json", &library)?;
    }
    Ok(library)
}

pub fn resolve_or_remember(store: &Store, identity: Identity) -> Result<Resolution> {
    identity.validate()?;
    let Some(stable_id) = identity.stable_id.clone() else {
        return Ok(Resolution {
            profile_key: identity.display_name.clone(),
            identity,
            binding_source: "display_name".into(),
            binding_revision: load(store)?.revision,
        });
    };

    let initial_name = identity.display_name.clone();
    let model_id = identity.model_id.clone();
    let library = write(store, |library| {
        if let Some(binding) = library.bindings.get_mut(&stable_id) {
            let changed = binding.display_name != initial_name || binding.model_id != model_id;
            if changed {
                binding.display_name = initial_name.clone();
                binding.model_id = model_id.clone();
                binding.updated_at_ms = crate::analysis::now_ms();
            }
            return Ok(changed);
        }
        let profile_key = unique_profile_key(library, &stable_id, &initial_name)?;
        library.bindings.insert(
            stable_id.clone(),
            Binding {
                stable_id: stable_id.clone(),
                model_id: model_id.clone(),
                display_name: initial_name.clone(),
                profile_key,
                updated_at_ms: crate::analysis::now_ms(),
            },
        );
        Ok(true)
    })?;
    let binding = library
        .bindings
        .get(&stable_id)
        .context("Stable device binding was not persisted")?;
    Ok(Resolution {
        profile_key: binding.profile_key.clone(),
        identity,
        binding_source: "stable_id".into(),
        binding_revision: library.revision,
    })
}

pub fn bind_profile_key(
    store: &Store,
    expected_revision: Option<u64>,
    stable_id: &str,
    profile_key: &str,
) -> Result<Library> {
    valid_string(stable_id, 512, "stable_id")?;
    valid_string(profile_key, 256, "profile_key")?;
    let listening = crate::tuning::preferences::load(store)?;
    write(store, |library| {
        if let Some(expected) = expected_revision {
            ensure!(
                library.revision == expected,
                "Device binding revision conflict: expected {expected}, current {}",
                library.revision
            );
        }
        let binding = library
            .bindings
            .get_mut(stable_id)
            .context("Unknown stable device ID; connect the device before rebinding")?;
        if binding.profile_key == profile_key {
            return Ok(false);
        }
        ensure!(
            listening.devices.contains_key(profile_key),
            "Listening profile key '{profile_key}' does not exist; save that device profile before binding"
        );
        binding.profile_key = profile_key.to_owned();
        binding.updated_at_ms = crate::analysis::now_ms();
        Ok(true)
    })
}

pub fn inspect(store: &Store, stable_id: Option<&str>) -> Result<serde_json::Value> {
    let library = load(store)?;
    let binding = stable_id.and_then(|stable_id| library.bindings.get(stable_id));
    Ok(serde_json::json!({
        "revision": library.revision,
        "binding": binding,
        "count": library.bindings.len()
    }))
}
