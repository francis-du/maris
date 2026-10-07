//! The human and agent entry point for device-specific music preferences.
use crate::{
    audio,
    control::store::{self, Store, MAX_JSON_BYTES},
    dsp::music::MusicProfile,
    tuning::preferences as listening,
};
use anyhow::{ensure, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::{io::Read, path::PathBuf};
#[derive(Subcommand)]
pub enum Action {
    /// Show the selected device's saved listening settings and applied revision.
    Status,
    /// Show capability limits and correction evidence for the selected device.
    Capability,
    /// Match the selected device against the bundled AutoEq catalog.
    Match {
        #[arg(long)]
        /// Apply only an exact, unique AutoEq match.
        apply: bool,
    },
    /// Explicitly bind and apply one bundled AutoEq model to the selected device.
    Bind {
        /// Exact bundled AutoEq model name.
        model: String,
    },
    /// Print the JSON schema for device-specific listening settings.
    Schema,
    /// List original listening scenes, not measurement corrections.
    Scenes,
    /// Preview a device-constrained scene without changing audio or saved preferences.
    Preview {
        /// Preset name or stable ID.
        name: String,
    },
    /// Apply a device-specific listening preset or scene.
    Preset {
        /// Preset name or stable ID.
        name: String,
    },
    /// Edit device-specific listening parameters directly.
    Set {
        #[arg(long, allow_hyphen_values = true)]
        /// Bass gain in dB.
        bass: Option<f64>,
        #[arg(long, help = "Virtual-bass amount from 0 to 1; zero disables it")]
        /// Virtual bass amount.
        virtual_bass: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        /// Presence gain in dB.
        presence: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        /// High-frequency gain in dB.
        air: Option<f64>,
        #[arg(long)]
        /// High-frequency softness amount.
        softness: Option<f64>,
        #[arg(long)]
        /// Transient emphasis amount.
        intensity: Option<f64>,
        #[arg(
            long,
            help = "Adaptive dynamic-EQ strength from 0 to 1; zero disables it"
        )]
        /// Adaptive EQ strength.
        adaptive: Option<f64>,
        #[arg(long)]
        /// Stereo width amount.
        width: Option<f64>,
        #[arg(long, help = "Virtual 360 amount from 0 to 1")]
        /// Virtual surround amount.
        virtual_surround: Option<f64>,
        #[arg(long, help = "Stereo Focus amount from 0 to 1")]
        /// Stereo focus amount.
        stereo_focus: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        /// Stereo balance; negative favors left, positive favors right.
        balance: Option<f64>,
        #[arg(long,action=clap::ArgAction::Set)]
        /// Enable or disable listening compression.
        compressor: Option<bool>,
    },
    /// Switch between the enhanced path and its level-matched reference.
    Compare {
        #[arg(action=clap::ArgAction::Set)]
        /// Use the level-matched reference path: true or false.
        reference: bool,
    },
    /// Persist the active output's listening profile as a device profile.
    SaveDevice,
    /// Import a strict Equalizer APO/AutoEq parametric correction file.
    Import {
        /// Equalizer APO or AutoEq parametric correction file.
        file: PathBuf,
        #[arg(long)]
        /// Measurement source name recorded with the correction.
        source: String,
    },
    /// Validate or apply a device listening-profile JSON file.
    Apply {
        /// Settings JSON file to validate or apply.
        file: PathBuf,
        #[arg(long)]
        /// Validate or preview without applying changes.
        dry_run: bool,
    },
}
pub fn run(
    store: &Store,
    device: Option<String>,
    expected: Option<u64>,
    action: Action,
) -> Result<Value> {
    let runtime = audio::runtime_status(store);
    let explicit_device = device.is_some();
    let mutates = !matches!(
        &action,
        Action::Status
            | Action::Capability
            | Action::Schema
            | Action::Scenes
            | Action::Preview { .. }
            | Action::Apply { dry_run: true, .. }
    );
    if mutates && !explicit_device {
        ensure!(
            crate::ui::tui::studio::live(&runtime, crate::analysis::now_ms())
                && runtime["profile_key"]
                    .as_str()
                    .is_some_and(|key| !key.is_empty()),
            "No current output; pass --device default or an explicit device profile"
        );
    }
    if matches!(&action, Action::SaveDevice) {
        ensure!(
            device.as_deref() != Some("default"),
            "SaveDevice requires a device profile, not default"
        );
    }
    let active_output = runtime["output"].as_str().map(str::to_owned);
    let device = device.or_else(|| {
        if runtime["active"] == true {
            runtime["profile_key"]
                .as_str()
                .or_else(|| runtime["output"].as_str())
                .map(str::to_owned)
        } else {
            None
        }
    });
    let device = device.filter(|name| name != "default");
    let discovery_name = if explicit_device {
        device.clone()
    } else {
        active_output.clone().or_else(|| device.clone())
    };
    let library = listening::load(store)?;
    match action {
        Action::Status => {
            let capability = device
                .as_deref()
                .map(|name| crate::devices::capability::effective(store, name))
                .transpose()?;
            Ok(
                json!({"listening":library,"selected_device":device,"active_output":active_output,"device_identity":runtime["device_identity"],"profile_binding_source":runtime["profile_binding_source"],"device_capability":capability,"effective":device.as_deref().map_or(&library.default,|name|library.effective(name)),"applied_music_revision":runtime["applied_music_revision"],"engine_supports_music":runtime["music_processing"]==true,"engine_revision":runtime["engine_revision"],"restart_required":runtime["active"]==true && runtime["engine_revision"]!=crate::dsp::DSP_REVISION,"tonal_bypass":runtime["tonal_bypass"],"pending":runtime["applied_music_revision"].as_u64()!=Some(library.revision)}),
            )
        }
        Action::Capability => {
            let name = device
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("No current output device; pass --device"))?;
            crate::devices::capability::inspect(store, name)
        }
        Action::Match { apply } => {
            let key = device
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("No current output device; pass --device"))?;
            let name = discovery_name.as_deref().unwrap_or(key);
            let entries = crate::devices::autoeq::ensure_index(store)?;
            let matched = crate::devices::autoeq::match_device(name, &entries);
            crate::devices::capability::remember_match(store, key, &matched, false)?;
            let applied = if apply && matched.auto_apply {
                if let Some(entry) = matched.candidate.as_ref() {
                    crate::devices::autoeq::apply_entry(store, key, entry)?;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            Ok(
                json!({"match":matched,"applied":applied,"rule":"--apply only applies an exact unique model; use sound bind for an explicit variant"}),
            )
        }
        Action::Bind { model } => {
            let key = device
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("No current output device; pass --device"))?;
            let name = discovery_name.as_deref().unwrap_or(key);
            let entries = crate::devices::autoeq::ensure_index(store)?;
            let entry = entries
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(&model))
                .ok_or_else(|| anyhow::anyhow!("AutoEq model not found: {model}"))?;
            let matched = crate::devices::autoeq::Match {
                device_name: name.to_owned(),
                candidate: Some(entry.clone()),
                confidence: 1.0,
                exact: true,
                auto_apply: false,
                reason: "Explicit user binding".into(),
            };
            crate::devices::capability::remember_match(store, key, &matched, true)?;
            let state = crate::devices::autoeq::apply_entry(store, key, entry)?;
            Ok(json!({"bound":entry,"listening":state}))
        }
        Action::Scenes => Ok(
            json!({"scenes":crate::presets::scenes::SCENES,"count":crate::presets::scenes::SCENES.len()}),
        ),
        Action::Preview { name } => {
            let capability = device
                .as_deref()
                .map(|key| crate::devices::capability::effective(store, key))
                .transpose()?
                .unwrap_or_default();
            let before = device
                .as_deref()
                .map_or(&library.default, |key| library.effective(key));
            let preview = crate::presets::scenes::prepare(before, &capability, &name)?;
            Ok(
                json!({"preview":preview,"device":device,"expected_listening_revision":library.revision,"applied":false}),
            )
        }
        Action::Schema => Ok(serde_json::to_value(
            schemars::generate::SchemaSettings::draft07()
                .into_generator()
                .into_root_schema_for::<MusicProfile>(),
        )?),
        Action::Preset { name } => Ok(serde_json::to_value(listening::preset(
            store,
            expected,
            device.as_deref(),
            &name,
        )?)?),
        Action::Set {
            bass,
            virtual_bass,
            presence,
            air,
            softness,
            intensity,
            adaptive,
            width,
            virtual_surround,
            stereo_focus,
            balance,
            compressor,
        } => {
            ensure!(
                bass.is_some()
                    || virtual_bass.is_some()
                    || presence.is_some()
                    || air.is_some()
                    || softness.is_some()
                    || intensity.is_some()
                    || adaptive.is_some()
                    || width.is_some()
                    || virtual_surround.is_some()
                    || stereo_focus.is_some()
                    || balance.is_some()
                    || compressor.is_some(),
                "Select at least one sound parameter"
            );
            Ok(serde_json::to_value(listening::edit_if_changed(
                store,
                expected,
                device.as_deref(),
                |p| {
                    if let Some(v) = bass {
                        p.bass_db = v;
                    }
                    if let Some(v) = virtual_bass {
                        ensure!((0.0..=1.0).contains(&v), "virtual-bass must be in [0, 1]");
                        p.bass_assist.enabled = v > 0.0;
                        p.bass_assist.amount = v;
                    }
                    if let Some(v) = presence {
                        p.presence_db = v;
                    }
                    if let Some(v) = air {
                        p.air_db = v;
                    }
                    if let Some(v) = softness {
                        p.softness = v;
                    }
                    if let Some(v) = intensity {
                        p.intensity = v;
                    }
                    if let Some(v) = adaptive {
                        ensure!((0.0..=1.0).contains(&v), "adaptive must be in [0, 1]");
                        p.adaptive.enabled = v > 0.0;
                        p.adaptive.strength = v;
                    }
                    if let Some(v) = width {
                        p.width = v;
                    }
                    if let Some(v) = virtual_surround {
                        ensure!(
                            (0.0..=1.0).contains(&v),
                            "virtual-surround must be in [0, 1]"
                        );
                        p.virtual_surround = v;
                    }
                    if let Some(v) = stereo_focus {
                        ensure!((0.0..=1.0).contains(&v), "stereo-focus must be in [0, 1]");
                        p.stereo_focus = v;
                    }
                    if let Some(v) = balance {
                        p.balance = v;
                    }
                    if let Some(v) = compressor {
                        p.compressor.enabled = v;
                    }
                    Ok(())
                },
            )?)?)
        }
        Action::Compare { reference } => Ok(serde_json::to_value(listening::compare(
            store,
            expected,
            device.as_deref(),
            reference,
        )?)?),
        Action::SaveDevice => {
            let name = match device {
                Some(name) => name,
                None => listening::current_device(store)?,
            };
            let profile = library.effective(&name).clone();
            Ok(serde_json::to_value(listening::edit(
                store,
                expected,
                Some(&name),
                |p| {
                    *p = profile;
                    Ok(())
                },
            )?)?)
        }
        Action::Import { file, source } => {
            let mut text = String::new();
            std::fs::File::open(file)?
                .take(MAX_JSON_BYTES + 1)
                .read_to_string(&mut text)?;
            ensure!(
                text.len() as u64 <= MAX_JSON_BYTES,
                "PEQ file exceeds 64 KiB"
            );
            let correction = listening::import_peq(&text, &source)?;
            Ok(serde_json::to_value(listening::edit(
                store,
                expected,
                device.as_deref(),
                |p| {
                    p.correction = correction.correction;
                    p.correction_preamp_db = correction.correction_preamp_db;
                    p.correction_source = correction.correction_source;
                    Ok(())
                },
            )?)?)
        }
        Action::Apply { file, dry_run } => {
            let profile: MusicProfile = store::read_json(&file)?;
            profile.validate()?;
            if dry_run {
                Ok(json!({"valid":true,"profile":profile}))
            } else {
                Ok(serde_json::to_value(listening::edit(
                    store,
                    expected,
                    device.as_deref(),
                    |p| {
                        *p = profile;
                        Ok(())
                    },
                )?)?)
            }
        }
    }
}
