//! Single-page listening console with optional detail views.
//! Risky routing changes use explicit pickers and Enter; there are no hidden detail-page keymaps.

pub mod dashboard;
pub mod input;
pub mod inspector;
pub mod monitor;
pub mod music;
pub mod output_picker;
pub mod preset_picker;
pub mod presets;
pub mod settings;
pub mod studio;
pub mod view;

use crate::ui::tui::input::directional;
use crate::{
    audio::{self, DeviceInfo},
    control,
    control::store::Store,
    i18n::Notice,
    tuning::planner::{self as music_tuning, Proposal},
    ui::desktop,
    ui::tui::input::{
        self as control_panel, CursorMemory, Overlay, UndoTarget, Workspace, DASHBOARD_SOUND_ROWS,
    },
    ui::tui::view::{self as tui_view, Console},
};
mod actions;
#[cfg(test)]
use crate::ui::tui::input::EQ_ROW_START;
use actions::{
    adjust_sound, application_pids, application_state, captured_application_pids, source_args,
    toggle_reference,
};
use anyhow::{ensure, Result};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
    }
}

pub fn run(
    store: Store,
    input: Option<String>,
    output: Option<String>,
    play: Option<PathBuf>,
    no_tray: bool,
) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "TUI requires an interactive terminal; use the JSON CLI or MCP instead"
    );

    let mut notice = Notice::new("E EQ · M Apps · V Device · I Assist · H Health · Esc Studio");
    let mut language = crate::i18n::Watcher::new(&store);
    if !no_tray {
        if let Err(error) = desktop::ensure_running(&store) {
            notice = Notice::new("Tray unavailable: {error}. Console controls remain available.")
                .arg("error", error);
        }
    }

    let mut inventory = audio::console_devices()?;
    let mut inputs = directional(&inventory, "input");
    let mut outputs = directional(&inventory, "output");
    let mut applications = application_state();
    let mut mixer_state = crate::mixer::load(&store).ok();
    let mut input_index = control_panel::selected_device_index(&inputs, input.as_deref())?;
    let mut output_index = control_panel::selected_device_index(&outputs, output.as_deref())?
        .or_else(|| outputs.iter().position(|device| device.is_default));
    let mut pin_output = output.is_some();

    if input.is_some() {
        ensure!(input_index.is_some(), "Input device does not exist");
    }
    if output.is_some() {
        ensure!(output_index.is_some(), "Output device does not exist");
    }

    if let Err(error) = start_initial_audio(
        &store,
        play.as_ref(),
        input.as_ref(),
        input_index.and_then(|index| inputs.get(index)),
        if pin_output {
            output_index.and_then(|index| outputs.get(index))
        } else {
            None
        },
        &mut notice,
    ) {
        notice = Notice::error(format!("{error:#}"));
    }

    enable_raw_mode()?;
    let _restore = RestoreTerminal;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let presets = crate::presets::console_catalog();
    let models = crate::models::models_at(&store)?;
    let mut picker_outputs = Vec::<DeviceInfo>::new();
    let mut picker_guard: Option<control_panel::PickerGuard> = None;
    let mut press_gate = control_panel::PressGate::default();
    let mut undo_target = UndoTarget::Listening;
    let mut configuration = crate::ui::tui::settings::Editor::default();
    let mut config_pointer = crate::ui::tui::settings::PointerLatch::default();
    let mut workspace = Workspace::Now;
    let mut cursors = CursorMemory::default();
    let mut overlay = Overlay::None;
    let mut sound_row = 0_usize;
    let sound_scroll = 0_usize;
    let mut app_row = 0_usize;
    let mut pending_apps = Vec::<i32>::new();
    let mut pending_apps_dirty = false;
    let mut output_choice = if pin_output {
        output_index.map_or(0, |index| index + 1)
    } else {
        0
    };
    let mut preset_choice = 0_usize;
    let mut preset_revisions: Option<(u64, u64)> = None;
    let mut goal_index = 0_usize;
    let mut proposal: Option<Proposal> = None;
    let mut motion = crate::analysis::spectrum::Motion::default();
    let mut theme = crate::ui::theme::Tracker::default();
    let mut last_frame = Instant::now();
    let mut last_refresh = Instant::now();
    let mut last_control_result_ms = 0_u64;

    loop {
        if last_refresh.elapsed() >= Duration::from_secs(1) {
            language.refresh(&store);
            let previous_input = input_index.and_then(|index| inputs.get(index)).cloned();
            let previous_output = output_index.and_then(|index| outputs.get(index)).cloned();
            if let Ok(updated) = audio::console_devices() {
                inventory = updated;
                inputs = directional(&inventory, "input");
                outputs = directional(&inventory, "output");
                input_index =
                    control_panel::preserved_device_index(&inputs, previous_input.as_ref());
                let preserved =
                    control_panel::preserved_device_index(&outputs, previous_output.as_ref());
                if pin_output && previous_output.is_some() && preserved.is_none() {
                    pin_output = false;
                    output_choice = 0;
                    notice =
                        "Pinned output disappeared; Maris returned to system-default following."
                            .into();
                }
                output_index =
                    preserved.or_else(|| outputs.iter().position(|device| device.is_default));
            }
            let selected_pid = application_pids(&applications).get(app_row).copied();
            applications = application_state();
            let pids = application_pids(&applications);
            let next_mixer = crate::mixer::load(&store).ok();
            app_row = if actions::mixer_mode(&audio::runtime_status(&store)) {
                actions::preserve_mixer_row(mixer_state.as_ref(), next_mixer.as_ref(), app_row)
            } else {
                selected_pid
                    .and_then(|pid| pids.iter().position(|item| *item == pid))
                    .unwrap_or_else(|| app_row.min(pids.len().saturating_sub(1)))
            };
            mixer_state = next_mixer;
            last_refresh = Instant::now();
        }

        let snapshot = store.load()?;
        let mut runtime = audio::runtime_status(&store);
        runtime["mixer_strip_count"] =
            serde_json::json!(mixer_state.as_ref().map(|state| state.config.strips.len()));
        runtime["mixer_control"] = serde_json::to_value(&mixer_state)?;
        if runtime["active"] == true
            && matches!(
                runtime["output_mode"].as_str(),
                Some("follow_system_default" | "pinned")
            )
        {
            pin_output = runtime["output_mode"] == "pinned";
            output_index = control_panel::runtime_output_index(&outputs, &runtime);
        }
        if let Some(updated_at_ms) = runtime["last_control_result"]["updated_at_ms"].as_u64() {
            if updated_at_ms > last_control_result_ms {
                last_control_result_ms = updated_at_ms;
                if runtime["last_control_result"]["action"] == "select_applications"
                    && runtime["last_control_result"]["ok"] == true
                {
                    pending_apps_dirty = false;
                }
                notice = if runtime["last_control_result"]["ok"] == true {
                    Notice::new("Applied: {action}").label_arg(
                        "action",
                        match runtime["last_control_result"]["action"].as_str() {
                            Some("select_output") => "Select output",
                            Some("select_applications") => "Select applications",
                            _ => "Applied",
                        },
                    )
                } else {
                    Notice::error(
                        runtime["last_control_result"]["error"]
                            .as_str()
                            .unwrap_or("Audio control failed."),
                    )
                };
            }
        }

        sync_application_scope(&runtime, &mut pending_apps, pending_apps_dirty);

        let frame_dt = last_frame.elapsed().as_secs_f64();
        motion.update(&runtime, frame_dt, crate::analysis::now_ms());
        let palette = theme.update(&runtime, frame_dt);
        last_frame = Instant::now();
        runtime["display_bands"] = serde_json::json!(motion.bands);
        runtime["display_levels"] = serde_json::json!(motion.levels);
        runtime["tonal_bypass"] = serde_json::json!(snapshot.profile.bypass);

        let listening = crate::tuning::preferences::load(&store)?;
        runtime["requested_music_revision"] = serde_json::json!(listening.revision);
        let active_profile_key = runtime["profile_key"]
            .as_str()
            .filter(|_| runtime["active"] == true)
            .map(str::to_owned);
        let selected_output_name = output_index
            .and_then(|index| outputs.get(index))
            .map(|device| device.name.clone());
        let profile_key = active_profile_key.or_else(|| selected_output_name.clone());
        let music = profile_key
            .as_deref()
            .map_or(&listening.default, |key| listening.effective(key));
        if let Some(key) = profile_key.as_deref() {
            if let Ok(capability) = crate::devices::capability::effective(&store, key) {
                runtime["device_capability"] = serde_json::to_value(capability)?;
            }
        }

        configuration.refresh(crate::ui::tui::settings::Context {
            runtime: &runtime,
            eq: &snapshot,
            listening: &listening,
        });
        if matches!(overlay, Overlay::Preset | Overlay::Output) {
            if let Some(guard) = &picker_guard {
                if guard
                    .validate(
                        &runtime,
                        listening.revision,
                        snapshot.revision,
                        crate::analysis::now_ms(),
                    )
                    .is_err()
                {
                    runtime["selection_invalidated"] = serde_json::json!(true);
                    notice = Notice::new("Configuration changed; reopen the picker");
                }
            }
        }
        let rendered_notice = notice.render();
        let view = Console {
            snapshot: &snapshot,
            configuration: Some(&configuration),
            runtime: &runtime,
            workspace,
            sound_row,
            home_row: if workspace == Workspace::Now {
                sound_row
            } else {
                cursors.home
            },
            sound_scroll,
            app_row,
            pending_apps: &pending_apps,
            devices: if overlay == Overlay::Output {
                &picker_outputs
            } else {
                &inventory
            },
            applications: &applications,
            selected_output: output_index.and_then(|index| outputs.get(index)),
            output_choice,
            presets: &presets,
            preset_choice,
            models: &models,
            music,
            palette,
            notice: &rendered_notice,
            goal: music_tuning::GOALS[goal_index],
            proposal: proposal.as_ref(),
            overlay,
        };
        let mut drawn_area = ratatui::layout::Rect::default();
        terminal.draw(|frame| {
            drawn_area = frame.area();
            tui_view::draw(frame, &view);
        })?;

        let Some(input) = control_panel::read_input()? else {
            continue;
        };
        let mut key = match input {
            Event::Key(key) => {
                config_pointer.reset();
                key
            }
            Event::Resize(_, _) => {
                config_pointer.reset();
                continue;
            }
            Event::Mouse(mouse) => {
                if matches!(
                    mouse.kind,
                    event::MouseEventKind::Down(_)
                        | event::MouseEventKind::ScrollUp
                        | event::MouseEventKind::ScrollDown
                ) {
                    press_gate.pointer_input();
                }
                let size = terminal.size()?;
                // Discard coordinates from a frame invalidated by resize. Modals and compact
                // mode cannot forward pointer events to controls hidden behind them.
                if size.width != drawn_area.width || size.height != drawn_area.height {
                    config_pointer.reset();
                    continue;
                }
                if overlay == Overlay::Preset {
                    config_pointer.reset();
                    if let Some(choice) = preset_picker::pointer_choice(drawn_area, &view, mouse) {
                        preset_choice = choice;
                    }
                    continue;
                }
                if overlay == Overlay::Output {
                    config_pointer.reset();
                    if let Some(choice) = output_picker::pointer_choice(
                        drawn_area,
                        picker_outputs.len().saturating_add(1),
                        output_choice,
                        runtime["selection_invalidated"] == true,
                        mouse,
                    ) {
                        output_choice = choice;
                    }
                    continue;
                }
                match config_pointer
                    .handle(drawn_area, &view, mouse)
                    .or_else(|| {
                        crate::ui::tui::studio::controls::pointer_action(
                            drawn_area, workspace, overlay, sound_row, mouse,
                        )
                    })
                    .or_else(|| {
                        if workspace == Workspace::Sound {
                            None
                        } else {
                            crate::ui::tui::inspector::pointer_action(drawn_area, &view, mouse)
                        }
                    }) {
                    Some(crate::ui::tui::studio::controls::PointerAction::SelectSound(index)) => {
                        sound_row = index;
                        continue;
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::Navigate(to)) => {
                        if configuration.pending() {
                            notice = "Apply or cancel this draft before leaving".into();
                            continue;
                        }
                        discard_application_draft(
                            workspace,
                            to,
                            &runtime,
                            &mut pending_apps,
                            &mut pending_apps_dirty,
                        );
                        navigate(&mut workspace, &mut sound_row, &mut cursors, to);
                        continue;
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::SelectHomeSound(
                        index,
                    )) => {
                        cursors.home = index;
                        if workspace == Workspace::Sound {
                            sound_row = index;
                        }
                        continue;
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::AdjustListening {
                        row,
                        direction,
                    }) => {
                        if let Err(error) = control_panel::ensure_displayed_output(&store, &runtime)
                        {
                            notice = Notice::error(error);
                            continue;
                        }
                        match adjust_sound(
                            &store,
                            profile_key.as_deref(),
                            listening.revision,
                            snapshot.revision,
                            row,
                            f64::from(direction),
                        ) {
                            Ok(Some(target)) => {
                                undo_target = target;
                                notice = "Saved; waiting for audio application".into();
                            }
                            Ok(None) => notice = "No changes".into(),
                            Err(error) => notice = Notice::error(format!("{error:#}")),
                        }
                        cursors.home = row;
                        continue;
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::SoundKey {
                        row,
                        key,
                    }) => {
                        sound_row = row;
                        KeyEvent::new(key, KeyModifiers::NONE)
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::PreviewGoal(index)) => {
                        if index >= music_tuning::GOALS.len() {
                            continue;
                        }
                        goal_index = index;
                        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::SelectApp(index)) => {
                        app_row = index;
                        continue;
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::ToggleApp(index)) => {
                        app_row = index;
                        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)
                    }
                    Some(crate::ui::tui::studio::controls::PointerAction::Key(code)) => {
                        KeyEvent::new(code, KeyModifiers::NONE)
                    }
                    None => continue,
                }
            }
            _ => continue,
        };
        if key.kind == KeyEventKind::Release {
            press_gate.accept(key, Instant::now());
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
        {
            break;
        }

        if let KeyCode::Char(ch) = key.code {
            key.code = KeyCode::Char(ch.to_ascii_lowercase());
        }
        if !press_gate.accept(key, Instant::now()) {
            continue;
        }
        if !configuration.permits(key.code) {
            notice = "Apply or cancel this draft before leaving".into();
            continue;
        }
        if overlay != Overlay::None {
            if runtime["selection_invalidated"] == true
                && !matches!(key.code, KeyCode::Esc | KeyCode::Char('o' | 'p'))
            {
                continue;
            }
            let size = terminal.size()?;
            if (size.width < 30 || size.height < 8) && key.code != KeyCode::Esc {
                notice = "Enlarge terminal to confirm. Esc cancels.".into();
                continue;
            }
            let result: Result<()> = (|| {
                match overlay {
                    Overlay::Help => {
                        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                            overlay = Overlay::None;
                        }
                    }
                    Overlay::Output => match key.code {
                        KeyCode::Esc | KeyCode::Char('o') => overlay = Overlay::None,
                        KeyCode::Up => output_choice = output_choice.saturating_sub(1),
                        KeyCode::Down => {
                            output_choice = (output_choice + 1).min(picker_outputs.len());
                        }
                        KeyCode::Enter => {
                            let selected = control_panel::confirmed_output(
                                &picker_outputs,
                                output_choice,
                                &outputs,
                            )?;
                            if runtime["active"] == true {
                                control_panel::ensure_displayed_output(&store, &runtime)?;
                            }
                            apply_output_choice(
                                &store,
                                &runtime,
                                selected,
                                &mut pin_output,
                                &mut output_index,
                                &outputs,
                                &mut notice,
                            )?;
                            overlay = Overlay::None;
                        }
                        _ => {}
                    },
                    Overlay::Preset => match key.code {
                        KeyCode::Esc | KeyCode::Char('p') => overlay = Overlay::None,
                        KeyCode::Up => preset_choice = preset_choice.saturating_sub(1),
                        KeyCode::Down => {
                            preset_choice =
                                (preset_choice + 1).min(presets.len().saturating_sub(1));
                        }
                        KeyCode::Enter => {
                            if let Some(preset) = presets.get(preset_choice) {
                                let rate = runtime["sample_rate"]
                                    .as_u64()
                                    .filter(|rate| (44_100..=192_000).contains(rate))
                                    .unwrap_or(48_000)
                                    as u32;
                                if matches!(preset.category, "scene" | "listening") {
                                    ensure!(
                                        !snapshot.profile.bypass,
                                        "Global bypass is active; enable processing first"
                                    );
                                    control_panel::ensure_displayed_output(&store, &runtime)?;
                                    let applied = crate::tuning::preferences::preset(
                                        &store,
                                        Some(listening.revision),
                                        profile_key.as_deref(),
                                        preset.id.strip_prefix("listening:").unwrap_or(&preset.id),
                                    )?;
                                    if applied.revision != listening.revision {
                                        undo_target = UndoTarget::Listening;
                                    }
                                } else {
                                    let applied =
                                        store.edit(Some(snapshot.revision), |current| {
                                            *current = crate::presets::apply_tone_curve(
                                                current, &preset.id, rate,
                                            )?;
                                            Ok(())
                                        })?;
                                    if applied.revision != snapshot.revision {
                                        undo_target = UndoTarget::Profile;
                                    }
                                }
                                notice = Notice::new("Preset applied: {preset}").label_arg(
                                    "preset",
                                    crate::i18n::preset_key(&preset.id, &preset.name),
                                );
                            }
                            overlay = Overlay::None;
                        }
                        _ => {}
                    },
                    Overlay::Proposal => match key.code {
                        KeyCode::Esc => {
                            proposal = None;
                            overlay = Overlay::None;
                        }
                        KeyCode::Enter => {
                            if let Some(plan) = proposal.take() {
                                match music_tuning::apply(&store, &plan) {
                                    Ok(_) => {
                                        if plan.before == plan.profile {
                                            notice = "No adjustment needed; the last undo remains available.".into();
                                        } else {
                                            undo_target = UndoTarget::Listening;
                                            notice =
                                                "Tuning applied. B compares; U undoes it.".into();
                                        }
                                    }
                                    Err(error) => notice = Notice::error(format!("{error:#}")),
                                }
                            }
                            overlay = Overlay::None;
                        }
                        _ => {}
                    },
                    Overlay::StartSystem => match key.code {
                        KeyCode::Esc => overlay = Overlay::None,
                        KeyCode::Enter => {
                            launch_system(
                                &store,
                                if pin_output {
                                    output_index.and_then(|index| outputs.get(index))
                                } else {
                                    None
                                },
                            )?;
                            overlay = Overlay::None;
                            notice = "Starting native system audio.".into();
                        }
                        _ => {}
                    },
                    Overlay::None => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                notice = Notice::error(format!("{error:#}"));
            }
            if overlay == Overlay::None && key.code == KeyCode::Enter {
                press_gate.shield_confirmation();
            }
            continue;
        }

        let size = terminal.size()?;
        if configuration.pending() && key.code == KeyCode::Esc {
            configuration.cancel();
            notice = "Draft cancelled; sound unchanged".into();
            continue;
        }
        if configuration.pending()
            && control_panel::compact_layout(size.width, size.height)
            && !matches!(key.code, KeyCode::Char('s'))
        {
            notice = "Draft pending · enlarge to apply · Esc cancels".into();
            continue;
        }
        if control_panel::compact_layout(size.width, size.height)
            && !control_panel::compact_key_allowed(key.code)
        {
            notice = "Enlarge the terminal to adjust sound; O opens the output picker.".into();
            continue;
        }
        if key.code == KeyCode::Char('q') {
            break;
        }
        if workspace == Workspace::Sound {
            use crate::ui::tui::settings::Outcome;
            match configuration.handle(
                &store,
                crate::ui::tui::settings::Context {
                    runtime: &runtime,
                    eq: &snapshot,
                    listening: &listening,
                },
                &mut sound_row,
                key.code,
            ) {
                Ok(Outcome::Pass) => {}
                Ok(Outcome::Browse) => continue,
                Ok(Outcome::Staged) => {
                    notice = if configuration.pending() {
                        "Draft only · sound unchanged"
                    } else {
                        "No changes"
                    }
                    .into();
                    continue;
                }
                Ok(Outcome::Cancelled) => {
                    notice = "Draft cancelled; sound unchanged".into();
                    continue;
                }
                Ok(Outcome::Applied(target)) => {
                    if let Some(target) = target {
                        undo_target = target;
                    }
                    notice = "Saved; waiting for audio application".into();
                    press_gate.shield_confirmation();
                    continue;
                }
                Err(error) => {
                    notice = Notice::error(error);
                    continue;
                }
            }
        }

        let destination = Workspace::direct(key.code).or_else(|| {
            if let KeyCode::Char(digit @ '1'..='6') = key.code {
                Workspace::from_digit(digit)
            } else {
                None
            }
        });
        if let Some(destination) = destination {
            discard_application_draft(
                workspace,
                destination,
                &runtime,
                &mut pending_apps,
                &mut pending_apps_dirty,
            );
            navigate(&mut workspace, &mut sound_row, &mut cursors, destination);
            continue;
        }

        let result: Result<()> = (|| {
            if workspace == Workspace::Apps {
                let before = pending_apps.clone();
                if let Some(message) = actions::application_key(
                    &store,
                    &runtime,
                    &applications,
                    &mut app_row,
                    &mut pending_apps,
                    if pin_output {
                        output_index.and_then(|index| outputs.get(index))
                    } else {
                        None
                    },
                    key.code,
                )? {
                    if pending_apps != before {
                        pending_apps_dirty = true;
                    }
                    mixer_state = crate::mixer::load(&store).ok();
                    notice = message;
                    return Ok(());
                }
            }
            if control_panel::changes_sound(workspace, key.code) {
                control_panel::ensure_displayed_output(&store, &runtime)?;
            }
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    let destination = if key.code == KeyCode::BackTab
                        || key.modifiers.contains(KeyModifiers::SHIFT)
                    {
                        workspace.previous()
                    } else {
                        workspace.next()
                    };
                    discard_application_draft(
                        workspace,
                        destination,
                        &runtime,
                        &mut pending_apps,
                        &mut pending_apps_dirty,
                    );
                    navigate(&mut workspace, &mut sound_row, &mut cursors, destination);
                }
                KeyCode::Char('?') => overlay = Overlay::Help,
                KeyCode::Char('o') => {
                    picker_guard = Some(control_panel::PickerGuard::capture(
                        &runtime,
                        listening.revision,
                        snapshot.revision,
                        crate::analysis::now_ms(),
                    ));
                    picker_outputs = outputs.clone();
                    output_choice = if pin_output {
                        output_index.map_or(0, |index| index + 1)
                    } else {
                        0
                    };
                    overlay = Overlay::Output;
                }
                KeyCode::Char('p') => {
                    picker_guard = Some(control_panel::PickerGuard::capture(
                        &runtime,
                        listening.revision,
                        snapshot.revision,
                        crate::analysis::now_ms(),
                    ));
                    let (eq_changed, listening_changed) = preset_revisions.map_or(
                        (false, false),
                        |(eq_revision, listening_revision)| {
                            (
                                eq_revision != snapshot.revision,
                                listening_revision != listening.revision,
                            )
                        },
                    );
                    preset_choice = preset_picker::preferred_choice(
                        &presets,
                        preset_choice,
                        &snapshot,
                        music,
                        &runtime,
                        eq_changed,
                        listening_changed,
                    )
                    .unwrap_or_else(|| preset_choice.min(presets.len().saturating_sub(1)));
                    preset_revisions = Some((snapshot.revision, listening.revision));
                    overlay = Overlay::Preset;
                }
                KeyCode::Char('b') => {
                    toggle_reference(&store, profile_key.as_deref(), listening.revision)?;
                }
                KeyCode::Char('l') => {
                    crate::i18n::cycle(&store)?;
                    notice = "Language changed.".into();
                }
                KeyCode::Char('s') => {
                    control::request_stop(&store)?;
                    notice = "Stop requested.".into();
                }
                KeyCode::Char('g')
                    if matches!(
                        workspace,
                        Workspace::Now | Workspace::Sound | Workspace::Intelligence
                    ) =>
                {
                    goal_index = (goal_index + 1) % music_tuning::GOALS.len();
                    notice = Notice::new("AI goal: {goal}")
                        .label_arg("goal", music_tuning::GOAL_LABELS[goal_index]);
                }
                KeyCode::Char('j')
                    if matches!(
                        workspace,
                        Workspace::Now | Workspace::Sound | Workspace::Intelligence
                    ) =>
                {
                    match music_tuning::from_live(&store, music_tuning::GOALS[goal_index]) {
                        Ok(plan) => {
                            proposal = Some(plan);
                            overlay = Overlay::Proposal;
                        }
                        Err(error) => notice = Notice::error(format!("{error:#}")),
                    }
                }
                KeyCode::Char('u') => {
                    match undo_target {
                        UndoTarget::Listening => {
                            let key = profile_key.as_deref().ok_or_else(|| {
                                anyhow::anyhow!("Missing current output identity")
                            })?;
                            crate::tuning::preferences::undo_device(
                                &store,
                                listening.revision,
                                key,
                            )?;
                        }
                        UndoTarget::Profile => {
                            store.undo(Some(snapshot.revision))?;
                        }
                    }
                    notice = "Previous change undone.".into();
                }
                KeyCode::Enter if workspace == Workspace::Now && runtime["active"] != true => {
                    overlay = Overlay::StartSystem;
                }
                KeyCode::Up if workspace == Workspace::Now => {
                    sound_row = sound_row.min(DASHBOARD_SOUND_ROWS - 1).saturating_sub(1);
                }
                KeyCode::Down if workspace == Workspace::Now => {
                    sound_row = (sound_row + 1).min(DASHBOARD_SOUND_ROWS - 1);
                }
                KeyCode::Left | KeyCode::Right if workspace == Workspace::Now => {
                    let direction = if key.code == KeyCode::Left { -1.0 } else { 1.0 };
                    let row = sound_row.min(DASHBOARD_SOUND_ROWS - 1);
                    if let Some(target) = adjust_sound(
                        &store,
                        profile_key.as_deref(),
                        listening.revision,
                        snapshot.revision,
                        row,
                        direction,
                    )? {
                        undo_target = target;
                        notice = "Saved; waiting for audio application".into();
                    } else {
                        notice = "No changes".into();
                    }
                }
                KeyCode::Char(' ') if workspace == Workspace::Now => {
                    toggle_reference(&store, profile_key.as_deref(), listening.revision)?;
                }
                KeyCode::Left | KeyCode::Right if workspace == Workspace::Intelligence => {
                    let count = music_tuning::GOALS.len();
                    goal_index = (goal_index
                        + if key.code == KeyCode::Left {
                            count - 1
                        } else {
                            1
                        })
                        % count;
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            notice = Notice::error(format!("{error:#}"));
        }
    }

    Ok(())
}

fn sync_application_scope(runtime: &serde_json::Value, pending: &mut Vec<i32>, dirty: bool) {
    if !dirty {
        *pending = if runtime["active"] == true {
            captured_application_pids(runtime)
        } else {
            Vec::new()
        };
    }
}

fn discard_application_draft(
    current: Workspace,
    destination: Workspace,
    runtime: &serde_json::Value,
    pending: &mut Vec<i32>,
    dirty: &mut bool,
) {
    if current == Workspace::Apps && destination != Workspace::Apps && *dirty {
        *pending = captured_application_pids(runtime);
        *dirty = false;
    }
}

fn navigate(workspace: &mut Workspace, row: &mut usize, cursors: &mut CursorMemory, to: Workspace) {
    *row = cursors.visit(*workspace, to, *row);
    *workspace = to;
}

fn start_initial_audio(
    store: &Store,
    play: Option<&PathBuf>,
    input_name: Option<&String>,
    input: Option<&DeviceInfo>,
    output: Option<&DeviceInfo>,
    notice: &mut Notice,
) -> Result<()> {
    if let Some(path) = play {
        let mut args = vec![
            "play".into(),
            path.canonicalize()?.to_string_lossy().into_owned(),
            "--repeat".into(),
        ];
        if let Some(output) = output {
            args.extend(["--output".into(), output.id.clone()]);
        }
        desktop::launch_audio(store, &args)?;
    } else if input_name.is_some() {
        desktop::launch_audio(store, &source_args("run", input, output))?;
    } else if control::is_stopped(store) {
        launch_system(store, output)?;
        *notice = "Starting native system audio.".into();
    }
    Ok(())
}

fn launch_system(store: &Store, output: Option<&DeviceInfo>) -> Result<()> {
    let mut args = source_args("system", None, output);
    args.push("--accept-routing".into());
    desktop::launch_audio(store, &args).map(|_| ())
}

fn apply_output_choice(
    store: &Store,
    runtime: &serde_json::Value,
    selected: Option<&DeviceInfo>,
    pin_output: &mut bool,
    output_index: &mut Option<usize>,
    outputs: &[DeviceInfo],
    notice: &mut Notice,
) -> Result<()> {
    if runtime["active"] == true && audio::native_controls(runtime) {
        control::request_output(store, selected.map(|device| device.id.as_str()))?;
        *notice = selected.map_or_else(
            || "Output request queued: follow system default.".into(),
            |device| Notice::new("Output request queued: {device}.").arg("device", &device.name),
        );
    } else if control::is_stopped(store) {
        launch_system(store, selected)?;
        *notice = selected.map_or_else(
            || "Starting on the system default output.".into(),
            |device| Notice::new("Starting on {device}.").arg("device", &device.name),
        );
    } else {
        anyhow::bail!("Live output switching requires native system audio");
    }
    *pin_output = selected.is_some();
    *output_index = selected
        .and_then(|device| outputs.iter().position(|item| item.id == device.id))
        .or_else(|| outputs.iter().position(|device| device.is_default));
    Ok(())
}

fn apply_application_scope(
    store: &Store,
    runtime: &serde_json::Value,
    pids: &[i32],
    output: Option<&DeviceInfo>,
) -> Result<()> {
    if runtime["active"] == true && audio::native_controls(runtime) {
        return control::request_applications(store, pids);
    }
    ensure!(
        control::is_stopped(store),
        "Application capture requires native system audio"
    );
    if pids.is_empty() {
        return launch_system(store, output);
    }
    let mut args = vec!["application".to_owned()];
    for pid in pids {
        args.extend(["--pid".into(), pid.to_string()]);
    }
    if let Some(output) = output {
        args.extend(["--output".into(), output.id.clone()]);
    }
    args.push("--accept-routing".into());
    desktop::launch_audio(store, &args).map(|_| ())
}

#[cfg(test)]
#[path = "../../../tests/unit/tui_actions.rs"]
mod tests;
