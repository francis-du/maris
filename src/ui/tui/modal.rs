//! Guarded modal actions, separated from terminal polling and periodic refresh.
#[cfg(test)]
#[path = "../../../tests/unit/picker_modal.rs"]
mod tests;
use super::{
    input::{self, Overlay, UndoTarget},
    picker_search::Search,
    session,
};
use crate::{
    audio::DeviceInfo,
    control::store::{Snapshot, Store},
    i18n::Notice,
    presets::PresetSummary,
    tuning::{
        planner::{self, Proposal},
        preferences,
    },
};
use anyhow::{ensure, Result};
use crossterm::event::KeyCode;
use serde_json::Value;

pub(super) struct Context<'a> {
    pub store: &'a Store,
    pub runtime: &'a Value,
    pub snapshot: &'a Snapshot,
    pub listening_revision: u64,
    pub profile_key: Option<&'a str>,
    pub outputs: &'a [DeviceInfo],
    pub picker_outputs: &'a [DeviceInfo],
    pub presets: &'a [PresetSummary],
}
pub(super) struct State<'a> {
    pub overlay: &'a mut Overlay,
    pub pin_output: &'a mut bool,
    pub output_index: &'a mut Option<usize>,
    pub output_choice: &'a mut usize,
    pub preset_choice: &'a mut usize,
    pub undo_target: &'a mut UndoTarget,
    pub proposal: &'a mut Option<Proposal>,
    pub notice: &'a mut Notice,
}

pub(super) fn handle(
    key: KeyCode,
    context: Context<'_>,
    state: State<'_>,
    search: &Search,
    page: usize,
) -> Result<()> {
    let Context {
        store,
        runtime,
        snapshot,
        listening_revision,
        profile_key,
        outputs,
        picker_outputs,
        presets,
    } = context;
    let State {
        overlay,
        pin_output,
        output_index,
        output_choice,
        preset_choice,
        undo_target,
        proposal,
        notice,
    } = state;
    let choices = search.choices(*overlay, picker_outputs, presets);
    let selection = match overlay {
        Overlay::Output => Some(&mut *output_choice),
        Overlay::Preset => Some(&mut *preset_choice),
        _ => None,
    };
    if let Some(selection) = selection {
        if key == KeyCode::Enter {
            if let Some(warning) = search.warning() {
                *notice = Notice::new(warning);
                return Ok(());
            }
        }
        if Search::browse(key, &choices, selection, page) {
            return Ok(());
        }
        if key == KeyCode::Enter && !choices.contains(selection) {
            *notice = Notice::new("No matches");
            return Ok(());
        }
        if key == KeyCode::Enter && page == 0 {
            *notice = Notice::new("Enlarge terminal to choose");
            return Ok(());
        }
    }
    match *overlay {
        Overlay::Help => {
            if matches!(key, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                *overlay = Overlay::None;
            }
        }
        Overlay::Output => match key {
            KeyCode::Esc | KeyCode::Char('o') => *overlay = Overlay::None,
            KeyCode::Enter => {
                let selected = input::confirmed_output(picker_outputs, *output_choice, outputs)?;
                if runtime["active"] == true {
                    input::ensure_displayed_output(store, runtime)?;
                }
                session::apply_output_choice(
                    store,
                    runtime,
                    selected,
                    pin_output,
                    output_index,
                    outputs,
                    notice,
                )?;
                *overlay = Overlay::None;
            }
            _ => {}
        },
        Overlay::Preset => match key {
            KeyCode::Esc | KeyCode::Char('p') => *overlay = Overlay::None,
            KeyCode::Enter => {
                if let Some(preset) = presets.get(*preset_choice) {
                    let rate = runtime["sample_rate"]
                        .as_u64()
                        .filter(|rate| (44_100..=192_000).contains(rate))
                        .unwrap_or(48_000) as u32;
                    if matches!(preset.category, "scene" | "listening") {
                        ensure!(
                            !snapshot.profile.bypass,
                            "Global bypass is active; enable processing first"
                        );
                        input::ensure_displayed_output(store, runtime)?;
                        let applied = preferences::preset(
                            store,
                            Some(listening_revision),
                            profile_key,
                            preset.id.strip_prefix("listening:").unwrap_or(&preset.id),
                        )?;
                        if applied.revision != listening_revision {
                            *undo_target = UndoTarget::Listening;
                        }
                    } else {
                        let applied = store.edit(Some(snapshot.revision), |current| {
                            *current = crate::presets::apply_tone_curve(current, &preset.id, rate)?;
                            Ok(())
                        })?;
                        if applied.revision != snapshot.revision {
                            *undo_target = UndoTarget::Profile;
                        }
                    }
                    *notice = Notice::new("Preset applied: {preset}")
                        .label_arg("preset", crate::i18n::preset_key(&preset.id, &preset.name));
                }
                *overlay = Overlay::None;
            }
            _ => {}
        },
        Overlay::Proposal => match key {
            KeyCode::Esc => {
                *proposal = None;
                *overlay = Overlay::None;
            }
            KeyCode::Enter => {
                if let Some(plan) = proposal.take() {
                    match planner::apply(store, &plan) {
                        Ok(_) => {
                            if plan.before == plan.profile {
                                *notice =
                                    "No adjustment needed; the last undo remains available.".into();
                            } else {
                                *undo_target = UndoTarget::Listening;
                                *notice = "Tuning applied. B compares; U undoes it.".into();
                            }
                        }
                        Err(error) => *notice = Notice::error(format!("{error:#}")),
                    }
                }
                *overlay = Overlay::None;
            }
            _ => {}
        },
        Overlay::StartSystem => match key {
            KeyCode::Esc => *overlay = Overlay::None,
            KeyCode::Enter => {
                session::launch_system(
                    store,
                    if *pin_output {
                        output_index.and_then(|index| outputs.get(index))
                    } else {
                        None
                    },
                )?;
                *overlay = Overlay::None;
                *notice = "Starting native system audio.".into();
            }
            _ => {}
        },
        Overlay::None => {}
    }
    Ok(())
}
