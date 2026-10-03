//! Validated live controls. The configuration editor shares the same pure adjustments.
use crate::{
    control::store::Store,
    tuning::preferences as listening,
    ui::tui::input::{UndoTarget, SOUND_ROWS},
};
use anyhow::{ensure, Result};

pub(super) fn source_args(
    command: &str,
    input: Option<&crate::audio::DeviceInfo>,
    output: Option<&crate::audio::DeviceInfo>,
) -> Vec<String> {
    let mut args = vec![command.to_owned()];
    if let Some(input) = input {
        args.extend(["--input".into(), input.id.clone()]);
    }
    if let Some(output) = output {
        args.extend(["--output".into(), output.id.clone()]);
    }
    args
}

pub(super) fn application_state() -> serde_json::Value {
    crate::audio::applications().unwrap_or_else(|error| {
        serde_json::json!({
            "available": false, "applications": [], "error": format!("{error:#}")
        })
    })
}

pub(super) fn captured_application_pids(runtime: &serde_json::Value) -> Vec<i32> {
    runtime["captured_application_pids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_i64)
        .filter_map(|pid| i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .collect()
}

pub(super) fn application_pids(state: &serde_json::Value) -> Vec<i32> {
    crate::ui::tui::input::visible_applications(state)
        .into_iter()
        .filter_map(|application| application["pid"].as_i64())
        .filter_map(|pid| i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .collect()
}

pub(super) fn mixer_mode(runtime: &serde_json::Value) -> bool {
    runtime["system_backend"] == "multi_device_mixer"
}

pub(super) fn mixer_rows(runtime: &serde_json::Value) -> &[serde_json::Value] {
    runtime["mixer_control"]["config"]["strips"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}

pub(super) fn preserve_mixer_row(
    before: Option<&crate::mixer::MixerState>,
    after: Option<&crate::mixer::MixerState>,
    row: usize,
) -> usize {
    let Some(after) = after else {
        return usize::MAX;
    };
    before
        .and_then(|state| state.config.strips.get(row))
        .and_then(|selected| {
            after
                .config
                .strips
                .iter()
                .position(|strip| strip.id == selected.id)
        })
        .unwrap_or(after.config.strips.len())
}

#[derive(Clone, Copy)]
pub(super) struct ApplicationInput {
    pub key: crossterm::event::KeyCode,
    pub now: u64,
}

/// The Apps page controls the active mixer by strip ID, not an application-list index.
/// Changing inputs or output assignments remains an explicit separate operation.
pub(super) fn application_key(
    store: &Store,
    runtime: &serde_json::Value,
    applications: &serde_json::Value,
    row: &mut usize,
    pending: &mut Vec<i32>,
    output: Option<&crate::audio::DeviceInfo>,
    key: crossterm::event::KeyCode,
) -> Result<Option<crate::i18n::Notice>> {
    application_key_at(
        store,
        runtime,
        applications,
        row,
        pending,
        output,
        ApplicationInput {
            key,
            now: crate::analysis::now_ms(),
        },
    )
}

#[doc(hidden)]
pub(super) fn application_key_at(
    store: &Store,
    runtime: &serde_json::Value,
    applications: &serde_json::Value,
    row: &mut usize,
    pending: &mut Vec<i32>,
    output: Option<&crate::audio::DeviceInfo>,
    input: ApplicationInput,
) -> Result<Option<crate::i18n::Notice>> {
    use crate::i18n::Notice;
    use crossterm::event::KeyCode;
    let ApplicationInput { key, now } = input;
    let mixing = mixer_mode(runtime);
    let count = if mixing {
        mixer_rows(runtime).len()
    } else {
        application_pids(applications).len()
    };
    if matches!(key, KeyCode::Up | KeyCode::Down) {
        *row = if key == KeyCode::Up {
            (*row).min(count).saturating_sub(1)
        } else {
            row.saturating_add(1).min(count.saturating_sub(1))
        };
        return Ok(Some(Notice::new(if mixing {
            "Mixer controls"
        } else {
            "Mark a scope, then apply once"
        })));
    }
    if mixing {
        if !matches!(
            key,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('+' | '-' | ' ' | 'x' | '[' | ']' | 'u')
                | KeyCode::Enter
        ) {
            return Ok(None);
        }
        if key == KeyCode::Enter {
            return Ok(Some(Notice::new(
                "Mixer controls apply immediately; assignments need a restart",
            )));
        }
        crate::ui::tui::input::ensure_displayed_output_at(store, runtime, now)?;
        let current = crate::audio::runtime_status_at(store, now);
        ensure!(mixer_mode(&current), "Audio session changed; select again");
        ensure!(
            current["mixer"]["restart_required"] == false,
            "Mixer assignments changed; stop and restart before adjusting channels"
        );
        let displayed: crate::mixer::MixerState =
            serde_json::from_value(runtime["mixer_control"].clone())?;
        displayed.config.validate()?;
        if key == KeyCode::Char('u') {
            crate::mixer::undo(store, Some(displayed.revision))?;
            return Ok(Some(Notice::new("Previous mixer change undone")));
        }
        let selected = displayed
            .config
            .strips
            .get(*row)
            .ok_or_else(|| anyhow::anyhow!("Choose a mixer channel first"))?;
        let updated = crate::mixer::edit(store, Some(displayed.revision), |config| {
            let strip = config
                .strips
                .iter_mut()
                .find(|strip| strip.id == selected.id)
                .ok_or_else(|| anyhow::anyhow!("Selected mixer channel is no longer available"))?;
            ensure!(*strip == *selected, "Mixer channel changed; select again");
            crate::ui::tui::input::ensure_displayed_output_at(store, runtime, now)?;
            match key {
                KeyCode::Left | KeyCode::Char('-') => {
                    strip.gain_db = (strip.gain_db - 0.5).max(-60.0)
                }
                KeyCode::Right | KeyCode::Char('+') => {
                    strip.gain_db = (strip.gain_db + 0.5).min(12.0)
                }
                KeyCode::Char(' ') => strip.mute = !strip.mute,
                KeyCode::Char('x') => strip.solo = !strip.solo,
                KeyCode::Char('[') => strip.pan = (strip.pan - 0.05).max(-1.0),
                KeyCode::Char(']') => strip.pan = (strip.pan + 0.05).min(1.0),
                _ => unreachable!(),
            }
            Ok(())
        })?;
        return Ok(Some(Notice::new(
            if updated.revision == displayed.revision {
                "No changes"
            } else {
                "Saved; waiting for audio application"
            },
        )));
    }
    match key {
        KeyCode::Char(' ') => {
            if let Some(pid) = application_pids(applications).get(*row).copied() {
                if let Some(index) = pending.iter().position(|item| *item == pid) {
                    pending.remove(index);
                } else {
                    pending.push(pid);
                    pending.sort_unstable();
                    pending.dedup();
                }
            }
            Ok(Some(
                Notice::new("Pending app scope: {count} selected.").arg("count", pending.len()),
            ))
        }
        KeyCode::Enter => {
            let available = application_pids(applications);
            ensure!(
                pending.iter().all(|pid| available.contains(pid)),
                "A selected application exited; update the selection before applying"
            );
            super::apply_application_scope(store, runtime, pending, output)?;
            Ok(Some(if pending.is_empty() {
                Notice::new("Returning to all system playback except Maris.")
            } else {
                Notice::new("Applying {count} selected application(s).").arg("count", pending.len())
            }))
        }
        KeyCode::Char('a') => {
            pending.clear();
            Ok(Some(Notice::new(
                "All system playback selected; Enter applies, Esc returns without switching.",
            )))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mixer_controls.rs"]
mod mixer_tests;

pub(super) fn toggle_reference(store: &Store, device: Option<&str>, revision: u64) -> Result<()> {
    ensure!(
        !store.load()?.profile.bypass,
        "Global bypass is active; enable processing first"
    );
    let library = listening::load(store)?;
    let profile = device.map_or(&library.default, |key| library.effective(key));
    listening::compare(store, Some(revision), device, !profile.reference)?;
    Ok(())
}

pub(super) fn adjust_sound(
    store: &Store,
    device: Option<&str>,
    listening_revision: u64,
    profile_revision: u64,
    row: usize,
    direction: f64,
) -> Result<Option<UndoTarget>> {
    ensure!(row < SOUND_ROWS, "Unknown sound control");
    ensure!(
        direction == -1.0 || direction == 1.0,
        "Adjustment direction must be -1 or 1"
    );
    if row < 10 || row == 26 || row == 27 {
        let music_row = match row {
            26 => 10,
            27 => 11,
            _ => row,
        };
        return Ok(crate::ui::tui::music::adjust(
            store,
            device,
            listening_revision,
            music_row,
            direction,
        )?
        .then_some(UndoTarget::Listening));
    }
    if row == 14 || row == 15 {
        let result =
            listening::edit_if_changed(store, Some(listening_revision), device, |profile| {
                if row == 14 {
                    profile.enabled = direction > 0.0;
                } else {
                    profile.level_match = direction > 0.0;
                }
                Ok(())
            })?;
        return Ok((result.revision != listening_revision).then_some(UndoTarget::Listening));
    }
    let result = store.edit(Some(profile_revision), |profile| {
        crate::ui::tui::settings::adjust_eq(profile, row, direction)
    })?;
    Ok((result.revision != profile_revision).then_some(UndoTarget::Profile))
}
