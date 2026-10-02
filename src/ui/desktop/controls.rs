//! Menu Bar quick controls. No callback, platform-device mutation or model code lives here.
//! A selection is an in-memory preview; explicit apply uses the shared validated control paths.
use crate::{
    audio::{self, DeviceInfo},
    control::store::{Snapshot, Store},
    devices::capability::{self as device_profile, Capability},
    dsp::music::{self, MusicProfile},
    i18n::{self, Notice},
    presets::scenes,
    tuning::preferences::{self as listening, Library},
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;

const PREVIEW_TTL_MS: u64 = 60_000;

pub const PRESET_GROUPS: &[(&str, &[&str])] = &[
    (
        "Everyday listening",
        &["natural", "warm", "vocal", "detail", "soft", "night"],
    ),
    ("Work and focus", &["focus", "long-listening"]),
    (
        "Dialogue and film",
        &["dialogue", "cinema", "night-dialogue"],
    ),
    (
        "Music and games",
        &[
            "acoustic-listening",
            "orchestral",
            "tight-bass",
            "game-clarity",
        ],
    ),
    ("Device listening", &["small-speakers"]),
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    session: String,
    output: String,
    key: String,
    uid: Option<String>,
    binding: Option<u64>,
    rebinds: Option<u64>,
    rate: u32,
}
impl Target {
    fn read(runtime: &Value, now: u64) -> Result<Self> {
        ensure!(
            crate::ui::tui::studio::live(runtime, now),
            "No current audio telemetry"
        );
        ensure!(
            runtime["music_processing"] == true,
            "Restart the audio engine to use quick controls"
        );
        let string = |field: &str| -> Result<String> {
            let value = runtime[field]
                .as_str()
                .context("Missing current output identity")?;
            ensure!(
                !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control),
                "Invalid current output identity"
            );
            Ok(value.to_owned())
        };
        let rate = runtime["sample_rate"]
            .as_u64()
            .context("Missing output sample rate")?;
        ensure!(
            (44_100..=192_000).contains(&rate),
            "Unsupported output sample rate"
        );
        Ok(Self {
            session: string("session_id")?,
            output: string("output")?,
            key: string("profile_key")?,
            uid: runtime["device_identity"]["stable_id"]
                .as_str()
                .map(str::to_owned),
            binding: runtime["device_binding_revision"].as_u64(),
            rebinds: runtime["rebind_count"].as_u64(),
            rate: rate as u32,
        })
    }
}

struct Current {
    runtime: Value,
    target: Target,
    listening: Library,
    eq: Snapshot,
    capability: Capability,
}
impl Current {
    fn read(store: &Store) -> Result<Self> {
        let runtime = audio::runtime_status(store);
        let target = Target::read(&runtime, crate::analysis::now_ms())?;
        Ok(Self {
            capability: device_profile::effective(store, &target.key)?,
            listening: listening::load(store)?,
            eq: store.load()?,
            runtime,
            target,
        })
    }
    fn profile(&self) -> &MusicProfile {
        self.listening.effective(&self.target.key)
    }
}

#[derive(Clone, Debug)]
struct Guard {
    target: Target,
    listening_revision: u64,
    eq_revision: u64,
    created_at_ms: u64,
}
impl Guard {
    fn new(current: &Current) -> Self {
        Self {
            target: current.target.clone(),
            listening_revision: current.listening.revision,
            eq_revision: current.eq.revision,
            created_at_ms: crate::analysis::now_ms(),
        }
    }
    fn validate(&self, current: &Current, now: u64) -> Result<()> {
        ensure!(
            now.checked_sub(self.created_at_ms)
                .is_some_and(|age| age <= PREVIEW_TTL_MS),
            "Selection expired; select again"
        );
        ensure!(
            self.target == current.target,
            "Active output changed after preview"
        );
        ensure!(
            self.listening_revision == current.listening.revision,
            "Listening profile changed after preview"
        );
        ensure!(
            self.eq_revision == current.eq.revision,
            "EQ changed after preview; select again"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Selection {
    Preset {
        id: String,
        before: Box<MusicProfile>,
        capability: Box<Capability>,
        preview: Box<scenes::Preview>,
    },
    Output {
        selector: Option<String>,
        label: String,
    },
}
#[derive(Clone, Debug)]
struct Pending {
    guard: Guard,
    selection: Selection,
}

/// State is owned by the menu control thread. A stale preview is never silently regenerated on Apply.
#[derive(Default)]
pub struct Controller {
    pending: Option<Pending>,
    displayed: Option<Guard>,
    observed: bool,
    pub notice: Option<Notice>,
}
impl Controller {
    /// Bind immediate native actions to the state used to draw their labels.
    pub fn observe(&mut self, summary: &Summary) {
        self.observed = true;
        self.displayed = summary.guard.clone();
    }
    fn check_displayed(&self, current: &Current) -> Result<()> {
        if self.observed {
            let guard = self
                .displayed
                .as_ref()
                .context("No current audio telemetry")?;
            guard.validate(current, crate::analysis::now_ms())?;
        }
        Ok(())
    }
    fn acknowledge(&mut self, current: &Current, eq_revision: u64, listening_revision: u64) {
        if self.displayed.is_some() {
            self.displayed = Some(Guard {
                target: current.target.clone(),
                eq_revision,
                listening_revision,
                created_at_ms: crate::analysis::now_ms(),
            });
        }
    }
    pub fn toggle_processing(&mut self, store: &Store) -> Result<()> {
        let current = Current::read(store)?;
        self.check_displayed(&current)?;
        let updated = store.edit(Some(current.eq.revision), |profile| {
            profile.bypass = !profile.bypass;
            Ok(())
        })?;
        self.acknowledge(&current, updated.revision, current.listening.revision);
        self.pending = None;
        self.notice = Some(Notice::new("Saved; waiting for audio application"));
        Ok(())
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.notice = Some(Notice::new("Selection cancelled; sound unchanged"));
    }
    pub fn select_preset(&mut self, store: &Store, id: &str) -> Result<()> {
        self.pending = None;
        let current = Current::read(store)?;
        self.check_displayed(&current)?;
        ensure!(
            !current.eq.profile.bypass,
            "Global bypass is active; enable processing first"
        );
        let preview = scenes::prepare(current.profile(), &current.capability, id)?;
        crate::dsp::Settings::compile(&current.eq.profile, current.target.rate)?
            .with_music(&preview.profile, current.target.rate)?;
        self.pending = Some(Pending {
            guard: Guard::new(&current),
            selection: Selection::Preset {
                id: id.to_owned(),
                before: Box::new(current.profile().clone()),
                capability: Box::new(current.capability),
                preview: Box::new(preview),
            },
        });
        self.notice = Some(Notice::new("Selection ready; review then apply"));
        Ok(())
    }
    pub fn select_output(
        &mut self,
        store: &Store,
        selector: Option<&str>,
        inventory: &[DeviceInfo],
    ) -> Result<()> {
        self.pending = None;
        let current = Current::read(store)?;
        self.check_displayed(&current)?;
        ensure!(
            crate::audio::native_controls(&current.runtime),
            "Live output switching requires native system audio"
        );
        validate_output_backend(selector, &current.runtime)?;
        let label = output_label(selector, inventory)?;
        self.pending = Some(Pending {
            guard: Guard::new(&current),
            selection: Selection::Output {
                selector: selector.map(str::to_owned),
                label,
            },
        });
        self.notice = Some(Notice::new("Selection ready; review then apply"));
        Ok(())
    }
    pub fn pending_valid(&self, store: &Store, inventory: &[DeviceInfo]) -> Result<bool> {
        let Some(pending) = &self.pending else {
            return Ok(false);
        };
        let current = Current::read(store)?;
        pending
            .guard
            .validate(&current, crate::analysis::now_ms())?;
        match &pending.selection {
            Selection::Preset {
                capability, before, ..
            } => {
                ensure!(
                    **capability == current.capability,
                    "Device capability changed after preview"
                );
                ensure!(
                    **before == *current.profile(),
                    "Listening profile changed after preview"
                );
            }
            Selection::Output { selector, label } => {
                validate_output_backend(selector.as_deref(), &current.runtime)?;
                ensure!(
                    output_label(selector.as_deref(), inventory)? == *label,
                    "Selected output changed; select again"
                );
            }
        }
        Ok(true)
    }
    pub fn apply(&mut self, store: &Store, inventory: &[DeviceInfo]) -> Result<()> {
        self.pending_valid(store, inventory)?;
        let pending = self.pending.as_ref().context("No pending selection")?;
        let current = Current::read(store)?;
        pending
            .guard
            .validate(&current, crate::analysis::now_ms())?;
        let mut saved_music_revision = current.listening.revision;
        match &pending.selection {
            Selection::Preset {
                id,
                before,
                capability,
                preview,
            } => {
                ensure!(
                    **capability == current.capability,
                    "Device capability changed after preview"
                );
                let rebuilt = scenes::prepare(current.profile(), capability, id)?;
                ensure!(
                    rebuilt.profile == preview.profile && rebuilt.changes == preview.changes,
                    "Tuning proposal was modified"
                );
                if preview.profile == **before {
                    self.notice = Some(Notice::new(
                        "No adjustment needed; the last undo remains available.",
                    ));
                } else {
                    let updated = listening::edit(
                        store,
                        Some(pending.guard.listening_revision),
                        Some(&current.target.key),
                        |profile| {
                            ensure!(
                                *profile == **before,
                                "Listening profile changed after preview"
                            );
                            let live = Target::read(
                                &audio::runtime_status(store),
                                crate::analysis::now_ms(),
                            )?;
                            ensure!(
                                live == pending.guard.target,
                                "Active output changed after preview"
                            );
                            *profile = rebuilt.profile;
                            Ok(())
                        },
                    )?;
                    saved_music_revision = updated.revision;
                    self.notice = Some(Notice::new("Saved; waiting for audio application"));
                }
            }
            Selection::Output { selector, .. } => {
                crate::control::request_output_for_session(
                    store,
                    &current.target.session,
                    selector.as_deref(),
                )?;
                self.notice = Some(Notice::new("Output request queued; waiting for the engine"));
            }
        }
        // Acknowledge only this transaction, not a fresh read that could accept someone else's edit.
        self.acknowledge(&current, current.eq.revision, saved_music_revision);
        self.pending = None;
        Ok(())
    }
    pub fn compare(&mut self, store: &Store) -> Result<()> {
        let current = Current::read(store)?;
        self.check_displayed(&current)?;
        ensure!(
            !current.eq.profile.bypass,
            "Global bypass is active; enable processing first"
        );
        let updated = listening::compare(
            store,
            Some(current.listening.revision),
            Some(&current.target.key),
            !current.profile().reference,
        )?;
        self.acknowledge(&current, current.eq.revision, updated.revision);
        self.pending = None;
        self.notice = Some(Notice::new("Saved; waiting for audio application"));
        Ok(())
    }
    pub fn undo(&mut self, store: &Store) -> Result<()> {
        let current = Current::read(store)?;
        self.check_displayed(&current)?;
        let updated =
            listening::undo_device(store, current.listening.revision, &current.target.key)?;
        self.acknowledge(&current, current.eq.revision, updated.revision);
        self.pending = None;
        self.notice = Some(Notice::new("Previous change undone."));
        Ok(())
    }
    pub fn preview_lines(&self) -> Vec<String> {
        let Some(pending) = &self.pending else {
            return vec![i18n::text("No pending selection").into()];
        };
        let mut lines = vec![format!(
            "{}: {}",
            i18n::text("Output"),
            pending.guard.target.output
        )];
        match &pending.selection {
            Selection::Preset { id, preview, .. } => {
                lines.push(format!(
                    "{}: {}",
                    i18n::text("Scene"),
                    i18n::preset_name(id, id)
                ));
                if let Some(scene) = scenes::find(id) {
                    lines.push(i18n::text(scene.description).into());
                }
                lines.push(i18n::text("Correction stays unchanged").into());
                if preview.compression_enabled {
                    lines.push(i18n::text("Compression enabled without makeup gain").into());
                }
                if preview.device_constraints_applied {
                    lines.push(i18n::text("Device limits applied").into());
                }
                lines.extend(preview.changes.iter().map(|change| i18n::change(change)));
                if preview.changes.is_empty() {
                    lines.push(i18n::text("No changes").into());
                }
            }
            Selection::Output { selector, label } => {
                lines.push(format!(
                    "{}: {}",
                    i18n::text("Selected output"),
                    if selector.is_none() {
                        i18n::text("Follow system default")
                    } else {
                        label
                    }
                ));
                lines.push(i18n::text("System volume and default output stay unchanged").into());
            }
        }
        lines
    }
    pub fn apply_title(&self) -> String {
        let Some(pending) = &self.pending else {
            return i18n::text("Apply selection").into();
        };
        let target = match &pending.selection {
            Selection::Preset { id, .. } => i18n::preset_name(id, id),
            Selection::Output { selector: None, .. } => i18n::text("Follow system default"),
            Selection::Output { label, .. } => label,
        };
        i18n::format("Apply: {selection}", &[("selection", target)])
    }
}

/// State used to draw each native output item; shared with offline regression tests.
pub fn output_item_state(summary: &Summary, runtime: &Value, device: &DeviceInfo) -> (bool, bool) {
    let enabled = summary.output_enabled
        && device.direction == "output"
        && valid_output_selector(&device.id)
        && validate_output_backend(Some(&device.id), runtime).is_ok();
    let active = enabled
        && runtime["output_mode"] == "pinned"
        && device
            .id
            .split_once(':')
            .is_some_and(|(_, id)| runtime["device_identity"]["stable_id"].as_str() == Some(id));
    (enabled, active)
}

fn validate_output_backend(selector: Option<&str>, runtime: &Value) -> Result<()> {
    let Some(selector) = selector else {
        return Ok(());
    };
    let prefix = match runtime["system_backend"].as_str() {
        Some("coreaudio_process_tap") => "uid:",
        Some("pulse_server") => "pulse:",
        Some("wasapi_process_loopback") => "wasapi:",
        _ => anyhow::bail!("Live output switching requires native system audio"),
    };
    ensure!(
        selector
            .strip_prefix(prefix)
            .is_some_and(|id| !id.is_empty()),
        "Output identity does not match the active audio backend"
    );
    Ok(())
}

fn valid_output_selector(selector: &str) -> bool {
    ["uid:", "pulse:", "wasapi:"].iter().any(|prefix| {
        selector
            .strip_prefix(prefix)
            .is_some_and(|id| !id.is_empty())
    }) && selector.len() <= 516
        && !selector.chars().any(char::is_control)
}

fn output_label(selector: Option<&str>, inventory: &[DeviceInfo]) -> Result<String> {
    let Some(selector) = selector else {
        return Ok("Follow system default".into());
    };
    ensure!(
        valid_output_selector(selector),
        "Stable output identity required"
    );
    let mut found = inventory
        .iter()
        .filter(|device| device.direction == "output" && device.id == selector);
    let device = found
        .next()
        .context("Selected output disconnected; select again")?;
    ensure!(found.next().is_none(), "Ambiguous output identity");
    Ok(device.name.clone())
}

/// Match actual constrained preferences; a stale remembered name cannot mask a manual edit.
pub fn matching_preset(profile: &MusicProfile, capability: &Capability) -> Option<&'static str> {
    let mut matching = music::PRESETS.iter().copied().filter(|id| {
        scenes::prepare(profile, capability, id).is_ok_and(|mut expected| {
            expected.profile.reference = profile.reference;
            expected.profile == *profile
        })
    });
    let first = matching.next()?;
    matching.next().is_none().then_some(first)
}

#[derive(Debug)]
pub struct Summary {
    guard: Option<Guard>,
    pub current: bool,
    pub controls_enabled: bool,
    pub output_enabled: bool,
    pub undo_enabled: bool,
    pub preset_id: Option<&'static str>,
    pub status: &'static str,
    pub output: String,
    pub listening: String,
    pub eq: String,
    pub compare: String,
}
impl Summary {
    pub fn read(store: &Store, runtime: &Value, now: u64) -> Result<Self> {
        let current = crate::ui::tui::studio::live(runtime, now);
        let enabled = Target::read(runtime, now).is_ok();
        let eq = store.load()?;
        let library = listening::load(store)?;
        let key = runtime["profile_key"].as_str().filter(|_| current);
        let profile = key.map(|key| library.effective(key));
        let capability = key
            .map(|key| device_profile::effective(store, key))
            .transpose()?;
        let preset_id = profile
            .zip(capability.as_ref())
            .and_then(|(p, c)| matching_preset(p, c));
        let mixer_status = if current && runtime["system_backend"] == "multi_device_mixer" {
            crate::ui::tui::studio::controls::mixer_application_state(
                runtime,
                Some(crate::mixer::load(store)?.revision),
            )
        } else {
            None
        };
        let status = if !current {
            if runtime["stale"] == true || runtime["active"] == true {
                "Awaiting telemetry"
            } else {
                "Saved offline"
            }
        } else if runtime["settings_pending"] == true {
            "Pending audio update"
        } else if let Some(status) = mixer_status {
            status
        } else {
            match (
                runtime["applied_revision"].as_u64(),
                runtime["applied_music_revision"].as_u64(),
            ) {
                (Some(p), Some(m)) if p == eq.revision && m == library.revision => "Applied",
                (Some(_), Some(_)) => "Pending audio update",
                _ => "Apply state unknown",
            }
        };
        let label = preset_id.map_or(
            i18n::text(if current { "Custom" } else { "Unavailable" }),
            |id| i18n::preset_name(id, id),
        );
        Ok(Self {
            guard: Target::read(runtime, now).ok().map(|target| Guard {
                target,
                listening_revision: library.revision,
                eq_revision: eq.revision,
                created_at_ms: now,
            }),
            current,
            controls_enabled: enabled && !eq.profile.bypass,
            output_enabled: enabled && crate::audio::native_controls(runtime),
            undo_enabled: if let Some(key) = key.filter(|_| enabled) {
                listening::can_undo_device(store, &library, key)?
            } else {
                false
            },
            preset_id,
            status,
            output: format!(
                "{}: {}",
                i18n::text("Output"),
                if current {
                    runtime["output"].as_str().unwrap_or(i18n::text("Unknown"))
                } else {
                    i18n::text("Unavailable")
                }
            ),
            listening: format!(
                "{}: {} · {}",
                i18n::text("Listening preset"),
                label,
                i18n::text(status)
            ),
            eq: format!(
                "{}: {}",
                i18n::text("Tone curve"),
                i18n::preset_name(&eq.profile.name, &eq.profile.name)
            ),
            compare: format!(
                "A/B: {}",
                i18n::text(if eq.profile.bypass {
                    "Processing bypassed"
                } else if profile.is_some_and(|p| p.reference) {
                    "Reference A"
                } else if profile.is_some() {
                    "Enhanced B"
                } else {
                    "Unavailable"
                })
            ),
        })
    }
}

/// Native rows stay bounded while stored identifiers and detailed errors remain unchanged.
pub fn menu_text(value: &str) -> String {
    let text = crate::ui::theme::clean(value).replace('&', "&&");
    if text.chars().count() <= 76 {
        return text;
    }
    format!("{}…", text.chars().take(75).collect::<String>())
}

#[cfg(test)]
#[path = "../../../tests/unit/menu_controls.rs"]
mod tests;
