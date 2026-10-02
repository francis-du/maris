//! In-place Studio inspectors. Shared geometry drives rendering and pointer hit testing.
//! No audio mutation: actions return to the existing keyboard/revision/confirmation path.
use crate::{
    i18n::text as t,
    ui::theme::Palette,
    ui::tui::input::{self as control_panel, Overlay, Workspace},
    ui::tui::studio::controls::{self as controls, PointerAction},
    ui::tui::studio::{self as studio, fit},
    ui::tui::view::Console,
};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Paragraph},
    Frame,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct LayoutAreas {
    pub rail: Rect,
    pub work: Rect,
    pub mixer: Rect,
    pub health: Rect,
}
pub fn layout(area: Rect) -> LayoutAreas {
    let home = studio::layout(area);
    LayoutAreas {
        rail: home.rail,
        work: Rect::new(
            home.analyzer.x,
            home.rail.y,
            home.analyzer.width,
            home.rail.height,
        ),
        mixer: home.mixer,
        health: home.health,
    }
}
pub fn row(area: Rect, index: u16) -> Rect {
    Rect::new(
        area.x,
        area.y + index.min(area.height),
        area.width,
        u16::from(index < area.height),
    )
}
pub fn inset(area: Rect) -> Rect {
    let pad = 1.min(area.width / 2);
    Rect::new(
        area.x + pad,
        area.y,
        area.width.saturating_sub(pad * 2),
        area.height,
    )
}
fn contains(area: Rect, event: MouseEvent) -> bool {
    area.contains((event.column, event.row).into())
}
fn write(frame: &mut Frame<'_>, area: Rect, text: &str, style: Style) {
    if area.width > 0 && area.height > 0 {
        frame.render_widget(
            Paragraph::new(fit(text, area.width)).style(style),
            row(area, 0),
        );
    }
}
fn heading(frame: &mut Frame<'_>, area: Rect, label: &str, p: Palette) {
    write(
        frame,
        area,
        t(label),
        Style::default()
            .fg(p.secondary)
            .add_modifier(Modifier::BOLD),
    );
}
fn field(frame: &mut Frame<'_>, area: Rect, label: &str, value: &str, p: Palette) {
    let label_width = (area.width / 3).clamp(12, 28);
    let parts = Layout::horizontal([Constraint::Length(label_width), Constraint::Min(1)])
        .spacing(1)
        .split(area);
    write(frame, parts[0], t(label), Style::default().fg(p.muted));
    write(frame, parts[1], value, Style::default().fg(p.text));
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map_or_else(|| t("Unavailable").into(), str::to_owned)
}
fn number(value: &Value, suffix: &str, live: bool) -> String {
    value
        .as_f64()
        .filter(|v| live && v.is_finite())
        .map_or_else(|| t("Unavailable").into(), |v| format!("{v:.2}{suffix}"))
}
fn counter(value: &Value, live: bool) -> String {
    value
        .as_u64()
        .filter(|_| live)
        .map_or_else(|| t("Unavailable").into(), |v| v.to_string())
}
fn button(frame: &mut Frame<'_>, area: Rect, label: &str, p: Palette, enabled: bool) {
    frame.render_widget(
        Paragraph::new(fit(t(label), area.width))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if enabled { p.accent } else { p.muted })
                    .bg(p.selected),
            ),
        area,
    );
}
pub fn command_rects(area: Rect) -> [Rect; 3] {
    let parts = Layout::horizontal([Constraint::Ratio(1, 3); 3])
        .spacing(1)
        .split(row(area, area.height.saturating_sub(1)));
    std::array::from_fn(|i| parts[i])
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    if view.workspace == Workspace::Sound {
        return crate::ui::tui::settings::draw(frame, area, view);
    }
    let areas = layout(area);
    let rail_row = if view.workspace == Workspace::Sound && view.sound_row < 10 {
        view.sound_row
    } else {
        view.home_row
    };
    let rail_view = Console {
        sound_row: rail_row,
        ..*view
    };
    studio::draw_rail(frame, areas.rail, &rail_view);
    frame.render_widget(
        Block::default().style(Style::default().bg(view.palette.panel)),
        areas.work,
    );
    let work = inset(areas.work);
    match view.workspace {
        Workspace::Apps => draw_apps(frame, work, view),
        Workspace::Device => draw_device(frame, work, view),
        Workspace::Intelligence => draw_intelligence(frame, work, view),
        Workspace::System => draw_health(frame, work, view),
        Workspace::Now | Workspace::Sound => {}
    }
    studio::draw_status(
        frame,
        areas.mixer,
        areas.health,
        view,
        studio::live(view.runtime, crate::analysis::now_ms()),
    );
}

pub fn app_rows(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y + 5.min(area.height),
        area.width,
        area.height.saturating_sub(10),
    )
}
pub fn app_offset(selected: usize, count: usize, height: u16) -> usize {
    let visible = usize::from(height).max(1);
    selected
        .min(count.saturating_sub(1))
        .saturating_sub(visible / 2)
        .min(count.saturating_sub(visible))
}
fn draw_apps(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    if super::actions::mixer_mode(view.runtime) {
        return draw_mixer(frame, area, view);
    }
    let p = view.palette;
    let live = studio::live(view.runtime, crate::analysis::now_ms());
    heading(frame, row(area, 0), "Application routing", p);
    let scope = if !live {
        t("Unavailable")
    } else if view.runtime["capture_scope"] == "selected_processes" {
        t("Selected applications")
    } else {
        t("System playback")
    };
    write(
        frame,
        row(area, 1),
        &format!("{scope}  →  Maris  →  {}", text(&view.runtime["output"])),
        Style::default().fg(p.text),
    );
    write(
        frame,
        row(area, 3),
        t("Mark a scope, then apply once"),
        Style::default().fg(p.muted),
    );
    let apps = control_panel::visible_applications(view.applications);
    let list = app_rows(area);
    let start = app_offset(view.app_row, apps.len(), list.height);
    if apps.is_empty() {
        write(
            frame,
            list,
            t(if view.applications["available"] == true {
                "No audio applications"
            } else {
                "Unavailable"
            }),
            Style::default().fg(p.muted),
        );
    }
    for (index, app) in apps
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(list.height))
    {
        let pid = app["pid"].as_i64().unwrap_or_default();
        let marked = view.pending_apps.contains(&(pid as i32));
        let captured = live
            && view.runtime["captured_application_pids"]
                .as_array()
                .is_some_and(|pids| pids.iter().any(|p| p.as_i64() == Some(pid)));
        let rect = row(list, (index - start) as u16);
        if index == view.app_row {
            frame.render_widget(
                Block::default().style(Style::default().bg(p.selected)),
                rect,
            );
        }
        let parts = Layout::horizontal([
            Constraint::Length(5),
            Constraint::Min(1),
            Constraint::Length(8),
            Constraint::Length(14),
        ])
        .spacing(1)
        .split(rect);
        write(
            frame,
            parts[0],
            if marked { "[x] ◆" } else { "[ ]" },
            Style::default().fg(p.accent),
        );
        write(
            frame,
            parts[1],
            ["display_name", "name", "bundle_id"]
                .into_iter()
                .filter_map(|field| app[field].as_str())
                .find(|name| !name.trim().is_empty())
                .unwrap_or(t("Unknown")),
            Style::default().fg(p.text),
        );
        write(
            frame,
            parts[2],
            &pid.to_string(),
            Style::default().fg(p.muted),
        );
        write(
            frame,
            parts[3],
            &if captured {
                format!("● {}", t("Routed"))
            } else if app["running_output"] == true {
                t("Playing").into()
            } else {
                t("Idle").into()
            },
            Style::default().fg(if captured { p.meter } else { p.muted }),
        );
    }
    write(
        frame,
        row(area, area.height.saturating_sub(4)),
        &format!(
            "{}: {}  ·  {}",
            t("Pending selection"),
            view.pending_apps.len(),
            t("Enter applies once")
        ),
        Style::default().fg(p.text),
    );
    write(
        frame,
        row(area, area.height.saturating_sub(3)),
        t("Use mixer commands for separate app volume and outputs"),
        Style::default().fg(p.muted),
    );
    let buttons = command_rects(area);
    for (rect, label) in
        buttons
            .into_iter()
            .zip(["A All playback", "Enter Apply scope", "Esc Studio"])
    {
        button(frame, rect, label, p, true);
    }
}

fn draw_mixer(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let r = view.runtime;
    let live = studio::live(r, crate::analysis::now_ms());
    let editable = live && r["mixer"]["restart_required"] == false;
    let strips = super::actions::mixer_rows(r);
    heading(frame, row(area, 0), "Mixer controls", p);
    let status = if !live {
        "Awaiting telemetry"
    } else if !editable {
        "Mixer assignments changed; stop and restart before adjusting channels"
    } else if r["mixer_control"]["revision"].as_u64().is_some()
        && r["mixer_control"]["revision"] == r["mixer"]["processing_revision"]
    {
        "Applied"
    } else {
        "Pending audio update"
    };
    write(frame, row(area, 1), t(status), Style::default().fg(p.muted));
    write(
        frame,
        row(area, 3),
        t("Channel · volume · balance · solo · measured peak"),
        Style::default().fg(p.muted),
    );
    let list = app_rows(area);
    let start = app_offset(view.app_row, strips.len(), list.height);
    if strips.is_empty() {
        write(
            frame,
            list,
            t("No mixer channels"),
            Style::default().fg(p.muted),
        );
    }
    for (index, strip) in strips
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(list.height))
    {
        let rect = row(list, (index - start) as u16);
        let selected = index == view.app_row;
        if selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(p.selected)),
                rect,
            );
        }
        let parts = Layout::horizontal([
            Constraint::Length(5),
            Constraint::Min(1),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(9),
        ])
        .spacing(1)
        .split(rect);
        write(
            frame,
            parts[0],
            if strip["mute"] == true { "[x]" } else { "[ ]" },
            Style::default().fg(p.accent),
        );
        write(
            frame,
            parts[1],
            &text(&strip["name"]),
            Style::default().fg(p.text),
        );
        write(
            frame,
            parts[2],
            &number(&strip["gain_db"], " dB", true),
            Style::default().fg(p.text),
        );
        write(
            frame,
            parts[3],
            &number(&strip["pan"], "", true),
            Style::default().fg(p.muted),
        );
        write(
            frame,
            parts[4],
            if strip["solo"] == true {
                t("ON")
            } else {
                t("OFF")
            },
            Style::default().fg(p.secondary),
        );
        let id = strip["id"].as_str();
        let measured = r["mixer"]["strips"].as_array().and_then(|values| {
            let mut matching = values
                .iter()
                .filter(|item| id.is_some() && item["id"].as_str() == id);
            let first = matching.next()?;
            matching.next().is_none().then_some(first)
        });
        let peak = measured.map_or(&Value::Null, |item| &item["peak_dbfs"]);
        write(
            frame,
            parts[5],
            &number(peak, " dB", live && measured.is_some()),
            Style::default().fg(p.meter),
        );
    }
    if let Some(selected) = strips.get(view.app_row) {
        write(
            frame,
            row(area, area.height.saturating_sub(4)),
            &format!("{}: {}", t("Source"), text(&selected["source_device"])),
            Style::default().fg(p.muted),
        );
    }
    write(
        frame,
        row(area, area.height.saturating_sub(3)),
        t("←/→ volume · Space mute · X solo · [/] balance · U undo"),
        Style::default().fg(p.text),
    );
    for (rect, label) in command_rects(area)
        .into_iter()
        .zip(["− Volume", "Space Mute", "X Solo"])
    {
        button(frame, rect, label, p, editable);
    }
}

fn draw_device(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let r = view.runtime;
    heading(frame, row(area, 0), "Output route", p);
    field(frame, row(area, 2), "Active output", &text(&r["output"]), p);
    field(
        frame,
        row(area, 3),
        "Output mode",
        crate::i18n::label(r["output_mode"].as_str().unwrap_or(t("Unavailable"))),
        p,
    );
    field(
        frame,
        row(area, 4),
        "Profile key",
        &text(&r["profile_key"]),
        p,
    );
    heading(frame, row(area, 6), "Device correction", p);
    field(
        frame,
        row(area, 7),
        "Correction",
        view.music
            .correction_source
            .as_deref()
            .unwrap_or(t("No correction")),
        p,
    );
    field(
        frame,
        row(area, 8),
        "AutoEq",
        &text(&r["device_capability"]["model"]),
        p,
    );
    field(
        frame,
        row(area, 9),
        "Capability",
        crate::i18n::label(
            r["device_capability"]["device_class"]
                .as_str()
                .unwrap_or(t("Unknown")),
        ),
        p,
    );
    let limits = format!(
        "{} {} · {} {}",
        t("LF"),
        number(&r["device_capability"]["bass_floor_hz"], " Hz", true),
        t("HF"),
        number(&r["device_capability"]["treble_ceiling_hz"], " Hz", true)
    );
    field(frame, row(area, 10), "Physical limits", &limits, p);
    heading(frame, row(area, 12), "Device identity", p);
    field(
        frame,
        row(area, 13),
        "Stable ID",
        &text(&r["device_identity"]["stable_id"]),
        p,
    );
    if area.height >= 20 {
        field(
            frame,
            row(area, 14),
            "Model",
            &text(&r["device_identity"]["model_id"]),
            p,
        );
        field(
            frame,
            row(area, 15),
            "Binding source",
            crate::i18n::label(
                r["profile_binding_source"]
                    .as_str()
                    .unwrap_or(t("Unavailable")),
            ),
            p,
        );
        write(
            frame,
            row(area, 17),
            t("Selection does not change system volume"),
            Style::default().fg(p.muted),
        );
    }
    let buttons = command_rects(area);
    button(frame, buttons[0], "O Choose output", p, true);
    button(frame, buttons[2], "Esc Studio", p, true);
}

/// A goal click creates a preview through the ordinary planner, never an audio write.
pub fn goal_rects(area: Rect) -> [Rect; 4] {
    let parts = Layout::horizontal([Constraint::Ratio(1, 4); 4])
        .spacing(1)
        .split(row(area, 3));
    std::array::from_fn(|index| parts[index])
}

fn draw_intelligence(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let now = crate::analysis::now_ms();
    let evidence = crate::tuning::planner::preview_evidence(view.runtime, now);
    heading(frame, row(area, 0), "Listening Assist", p);
    write(
        frame,
        row(area, 1),
        t("Choose a goal and preview"),
        Style::default().fg(p.muted),
    );
    for (index, rect) in goal_rects(area).into_iter().enumerate() {
        let selected = view.goal == crate::tuning::planner::GOALS[index];
        frame.render_widget(
            Paragraph::new(fit(
                t(crate::tuning::planner::GOAL_LABELS[index]),
                rect.width,
            ))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if selected { p.accent } else { p.text })
                    .bg(p.selected)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            rect,
        );
    }
    let readiness = evidence.as_ref().map_or_else(
        |error| crate::i18n::diagnostic(&format!("{error:#}")),
        |_| t("Ready for preview").into(),
    );
    field(frame, row(area, 5), "State", &readiness, p);
    let signal = evidence.as_ref().map_or_else(
        |_| t("Waiting for signal").into(),
        |a| {
            format!(
                "RMS {:.1} dBFS · {} {:.1} dB · {} {:.2}",
                a.rms_dbfs,
                t("Crest"),
                a.crest_db,
                t("Stereo correlation"),
                a.stereo_correlation
            )
        },
    );
    write(frame, row(area, 6), &signal, Style::default().fg(p.muted));
    if area.height >= 18 {
        heading(frame, row(area, 8), "Built-in processing", p);
        field(
            frame,
            row(area, 9),
            "Device correction",
            view.music
                .correction_source
                .as_deref()
                .unwrap_or(t("No correction")),
            p,
        );
        let adaptive = if view.music.adaptive.enabled {
            format!("{:.0}%", view.music.adaptive.strength * 100.0)
        } else {
            t("OFF").into()
        };
        field(frame, row(area, 10), "Dynamic EQ", &adaptive, p);
        let bass = if view.music.bass_assist.enabled {
            format!("{:.0}%", view.music.bass_assist.amount * 100.0)
        } else {
            t("OFF").into()
        };
        field(frame, row(area, 11), "Bass Assist", &bass, p);
        field(
            frame,
            row(area, 12),
            "A/B",
            if view.music.reference {
                t("Reference A")
            } else {
                t("Enhanced B")
            },
            p,
        );
        let speech = view.models["models"].as_array().is_some_and(|models| {
            models
                .iter()
                .any(|model| model["id"] == "rnnoise" && model["usable"] == true)
        });
        field(
            frame,
            row(area, 13),
            "Speech only",
            if speech {
                t("Included")
            } else {
                t("Not included")
            },
            p,
        );
    }
    if area.height >= 23 {
        heading(frame, row(area, 16), "Signal analysis", p);
        let context = evidence
            .as_ref()
            .ok()
            .map(|a| crate::tuning::planner::live_context(view.runtime, a, now));
        let label = context
            .as_ref()
            .filter(|c| c.mode == "semantic")
            .map_or_else(
                || t("Local signal-guided tuning").into(),
                |c| format!("{} · {}", c.model, c.top_genre().unwrap_or(t("Unknown"))),
            );
        write(frame, row(area, 17), &label, Style::default().fg(p.text));
        write(
            frame,
            row(area, 18),
            t("No model download needed"),
            Style::default().fg(p.muted),
        );
    }
    write(
        frame,
        row(area, area.height.saturating_sub(2)),
        t("Preview before apply · U restores the last change"),
        Style::default().fg(p.muted),
    );
    for (rect, label) in
        command_rects(area)
            .into_iter()
            .zip(["J Preview tuning", "B Compare", "U Undo"])
    {
        button(frame, rect, label, p, true);
    }
}

fn draw_health(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let r = view.runtime;
    let live = studio::live(r, crate::analysis::now_ms());
    heading(frame, row(area, 0), "Engine health", p);
    let records = [
        (
            "Backend",
            crate::i18n::label(r["system_backend"].as_str().unwrap_or(t("Unavailable"))).to_owned(),
        ),
        (
            "Capture",
            crate::i18n::label(r["capture_scope"].as_str().unwrap_or(t("Unavailable"))).to_owned(),
        ),
        (
            "Sample rate",
            if live {
                r["sample_rate"]
                    .as_u64()
                    .map_or_else(|| t("Unavailable").into(), |rate| format!("{rate} Hz"))
            } else {
                t("Unavailable").into()
            },
        ),
        ("Underruns", counter(&r["underruns"], live)),
        ("Overruns", counter(&r["overruns"], live)),
        (
            "Callback budget",
            number(
                &r["performance"]["callback_processing_average_percent"],
                "%",
                live,
            ),
        ),
        (
            "Callback maximum",
            number(&r["performance"]["callback_processing_max_us"], " us", live),
        ),
        (
            "Reported latency",
            number(&r["reported_output_path_latency_ms"], " ms", live),
        ),
        (
            "Analysis queue",
            counter(&r["performance"]["analysis_queue_depth"], live),
        ),
        ("Rebinds", counter(&r["rebind_count"], live)),
        ("Applied revision", counter(&r["applied_revision"], live)),
        (
            "Listening revision",
            counter(&r["applied_music_revision"], live),
        ),
    ];
    for (index, (label, value)) in records.iter().enumerate() {
        field(frame, row(area, 2 + index as u16), label, value, p);
    }
    if area.height >= 21 {
        heading(frame, row(area, 16), "Last audio command", p);
        field(
            frame,
            row(area, 17),
            "State",
            crate::i18n::label(
                r["last_control_result"]["action"]
                    .as_str()
                    .unwrap_or(t("Unavailable")),
            ),
            p,
        );
        if r["last_control_result"]["ok"] == false {
            write(
                frame,
                row(area, 18),
                &crate::i18n::diagnostic(
                    r["last_control_result"]["error"]
                        .as_str()
                        .unwrap_or(t("Unavailable")),
                ),
                Style::default().fg(p.warning),
            );
        }
    }
    write(
        frame,
        row(area, area.height.saturating_sub(2)),
        t("Callback budget is wall time, not CPU usage"),
        Style::default().fg(p.muted),
    );
    button(frame, command_rects(area)[2], "Esc Studio", p, true);
}

pub fn pointer_action(area: Rect, view: &Console<'_>, event: MouseEvent) -> Option<PointerAction> {
    if view.overlay != Overlay::None
        || view.workspace == Workspace::Now
        || control_panel::compact_layout(area.width, area.height)
        || !event.modifiers.is_empty()
    {
        return None;
    }
    if view.workspace == Workspace::Sound {
        return crate::ui::tui::settings::pointer_action(area, view, event);
    }
    let areas = layout(controls::shell(area).content);
    let rail_row = if view.workspace == Workspace::Sound && view.sound_row < 10 {
        view.sound_row
    } else {
        view.home_row
    };
    let rail = controls::rail(areas.rail, rail_row);
    if event.kind == MouseEventKind::Down(MouseButton::Left) {
        for index in rail.first..rail.first + rail.count {
            let rect = rail.parameter(index)?;
            if contains(rect, event) {
                if index == rail_row {
                    let parts = controls::parameter(rect);
                    for (target, direction) in [(parts.decrement, -1), (parts.increment, 1)] {
                        if contains(target, event) {
                            return Some(PointerAction::AdjustListening {
                                row: index,
                                direction,
                            });
                        }
                    }
                }
                return Some(PointerAction::SelectHomeSound(index));
            }
        }
    }
    let work = inset(areas.work);
    match view.workspace {
        Workspace::Intelligence if event.kind == MouseEventKind::Down(MouseButton::Left) => {
            for (index, rect) in goal_rects(work).into_iter().enumerate() {
                if contains(rect, event) {
                    return Some(PointerAction::PreviewGoal(index));
                }
            }
        }
        Workspace::Apps => {
            let list = app_rows(work);
            let count = if super::actions::mixer_mode(view.runtime) {
                super::actions::mixer_rows(view.runtime).len()
            } else {
                control_panel::visible_applications(view.applications).len()
            };
            if event.kind == MouseEventKind::Down(MouseButton::Left) && contains(list, event) {
                let index =
                    app_offset(view.app_row, count, list.height) + usize::from(event.row - list.y);
                if index < count {
                    return Some(if event.column < list.x + 5 {
                        PointerAction::ToggleApp(index)
                    } else {
                        PointerAction::SelectApp(index)
                    });
                }
            }
            if contains(list, event) {
                match event.kind {
                    MouseEventKind::ScrollUp => return Some(PointerAction::Key(KeyCode::Up)),
                    MouseEventKind::ScrollDown => return Some(PointerAction::Key(KeyCode::Down)),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    if event.kind == MouseEventKind::Down(MouseButton::Left) {
        let keys = match view.workspace {
            Workspace::Apps if super::actions::mixer_mode(view.runtime) => {
                if !studio::live(view.runtime, crate::analysis::now_ms())
                    || view.runtime["mixer"]["restart_required"] != false
                {
                    return None;
                }
                [
                    Some(KeyCode::Left),
                    Some(KeyCode::Char(' ')),
                    Some(KeyCode::Char('x')),
                ]
            }
            Workspace::Apps => [
                Some(KeyCode::Char('a')),
                Some(KeyCode::Enter),
                Some(KeyCode::Esc),
            ],
            Workspace::Device => [Some(KeyCode::Char('o')), None, Some(KeyCode::Esc)],
            Workspace::Intelligence => [
                Some(KeyCode::Char('j')),
                Some(KeyCode::Char('b')),
                Some(KeyCode::Char('u')),
            ],
            Workspace::System => [None, None, Some(KeyCode::Esc)],
            _ => [None; 3],
        };
        for (rect, key) in command_rects(work).into_iter().zip(keys) {
            if contains(rect, event) {
                return key.map(PointerAction::Key);
            }
        }
    }
    None
}
