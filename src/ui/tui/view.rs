//! Single-page Maris console with optional focused detail views.
//! Common actions stay in one footer; risky routing actions use explicit pickers.

use crate::{
    audio::DeviceInfo,
    control::store::Snapshot,
    dsp::Coefficients,
    i18n::text as t,
    presets::PresetSummary,
    tuning::planner::Proposal,
    ui::theme::Palette,
    ui::tui::input::{Overlay, Workspace},
};
use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};
use serde_json::Value;

#[derive(Clone, Copy)]
pub struct Console<'a> {
    pub snapshot: &'a Snapshot,
    pub configuration: Option<&'a crate::ui::tui::settings::Editor>,
    pub runtime: &'a Value,
    pub workspace: Workspace,
    pub sound_row: usize,
    pub home_row: usize,
    pub sound_scroll: usize,
    pub app_row: usize,
    pub pending_apps: &'a [i32],
    pub devices: &'a [DeviceInfo],
    pub applications: &'a Value,
    pub selected_output: Option<&'a DeviceInfo>,
    pub output_choice: usize,
    pub presets: &'a [PresetSummary],
    pub preset_choice: usize,
    pub models: &'a Value,
    pub music: &'a crate::dsp::music::MusicProfile,
    pub palette: Palette,
    pub notice: &'a str,
    pub goal: &'a str,
    pub proposal: Option<&'a Proposal>,
    pub overlay: Overlay,
}

impl Console<'_> {
    pub fn sample_rate(&self) -> u32 {
        self.runtime["sample_rate"]
            .as_u64()
            .filter(|rate| (44_100..=192_000).contains(rate))
            .unwrap_or(48_000) as u32
    }
}

pub fn response(profile: &crate::dsp::profile::Profile, rate: u32) -> Vec<f64> {
    let coefficients = profile.bands.map(|band| Coefficients::peaking(band, rate));
    (0..180)
        .map(|index| {
            if profile.bypass {
                return 0.0;
            }
            let log_hz = 20.0_f64.log10() + index as f64 / 179.0 * 3.0;
            let hz = 10.0_f64.powf(log_hz);
            coefficients.iter().map(|c| c.response_db(hz, rate)).sum()
        })
        .collect()
}

pub fn popup(area: Rect, lines: &[Line<'_>]) -> Rect {
    let desired = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .saturating_add(4)
        .clamp(30, 66);
    let width = area.width.saturating_sub(2).min(desired as u16);
    let inner = usize::from(width.saturating_sub(4)).max(1);
    let rows = lines
        .iter()
        .map(|line| line.width().div_ceil(inner).max(1))
        .sum::<usize>();
    let height =
        (rows.saturating_add(4).min(u16::MAX as usize) as u16).min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub fn draw(frame: &mut Frame<'_>, view: &Console<'_>) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(view.palette.bg).fg(view.palette.text)),
        area,
    );

    if crate::ui::tui::input::compact_layout(area.width, area.height) {
        crate::ui::tui::monitor::compact_colored(
            frame,
            area,
            view.runtime,
            view.snapshot.profile.bypass,
            view.palette,
        );
        if area.height >= 3 {
            let footer = Rect::new(area.x, area.bottom() - 2, area.width, 2);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(t(
                        if view.configuration.is_some_and(|editor| editor.pending()) {
                            "Draft pending · enlarge to apply · Esc cancels"
                        } else {
                            "O output · P preset · J AI · S stop · Q close"
                        },
                    )),
                    Line::from(crate::ui::theme::clean(view.notice)),
                ])
                .style(Style::default().fg(view.palette.muted)),
                footer,
            );
        }
        draw_overlay(frame, area, view);
        crate::ui::tui::studio::appearance::Appearance::from_environment()
            .apply(frame.buffer_mut());
        return;
    }

    let shell = crate::ui::tui::studio::controls::shell(area);
    draw_header(frame, shell.header, view);
    draw_actions(frame, shell.actions, view);
    crate::ui::tui::dashboard::draw(frame, shell.content, view);
    draw_footer(frame, shell.footer, view);
    draw_overlay(frame, area, view);
    crate::ui::tui::studio::appearance::Appearance::from_environment().apply(frame.buffer_mut());
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let now = crate::analysis::now_ms();
    let active = crate::ui::tui::studio::live(view.runtime, now);
    let pending = view.configuration.is_some_and(|editor| editor.pending());
    let settings = view.workspace == Workspace::Sound;
    let p = view.palette;
    let regions = crate::ui::tui::studio::controls::header(area);
    let output = view.runtime["output"]
        .as_str()
        .or_else(|| view.selected_output.map(|device| device.name.as_str()))
        .unwrap_or(t("System default"));
    let rate = view.runtime["sample_rate"]
        .as_u64()
        .filter(|_| active)
        .map_or_else(|| t("Unavailable").to_owned(), |rate| format!("{rate} Hz"));
    let mode = if active {
        "LIVE"
    } else if view.runtime["active"] == true {
        "STALE"
    } else {
        "STANDBY"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " MARIS ",
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", t(mode)),
                Style::default()
                    .fg(if active || mode == "STALE" {
                        p.bg
                    } else {
                        p.muted
                    })
                    .bg(if active {
                        p.meter
                    } else if mode == "STALE" {
                        p.warning
                    } else {
                        p.panel
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        regions.brand,
    );
    frame.render_widget(
        Paragraph::new(crate::ui::tui::studio::fit(
            &format!("{} {output}", if pending { "" } else { "O" }),
            regions.output.width,
        ))
        .style(Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
        regions.output,
    );
    let reference = if view.snapshot.profile.bypass {
        t("Processing bypassed")
    } else if view.music.reference {
        t("Reference A")
    } else {
        t("Enhanced B")
    };
    frame.render_widget(
        Paragraph::new(crate::ui::tui::studio::fit(
            &format!("{} {reference} · {rate}", if settings { "" } else { "B" }),
            regions.compare.width,
        ))
        .alignment(Alignment::Right)
        .style(Style::default().fg(if settings { p.muted } else { p.secondary })),
        regions.compare,
    );
    let preset = format!(
        " {} · {} {}",
        t(view.workspace.title()),
        if pending { t("Tone curve") } else { "P" },
        crate::i18n::preset_name(&view.snapshot.profile.name, &view.snapshot.profile.name)
    );
    frame.render_widget(
        Paragraph::new(crate::ui::tui::studio::fit(&preset, regions.preset.width))
            .style(Style::default().fg(p.muted)),
        regions.preset,
    );
    let gain = view.runtime["effective_preamp_db"]
        .as_f64()
        .filter(|v| v.is_finite() && active)
        .map_or_else(|| t("Unavailable").to_owned(), |v| format!("{v:+.1} dB"));
    let application = crate::ui::tui::studio::controls::application_state(
        view.runtime,
        view.snapshot.revision,
        now,
    );
    let draft_pending = view.configuration.is_some_and(|editor| editor.pending());
    let status = if draft_pending {
        t("Draft only · sound unchanged").to_owned()
    } else {
        format!("{} {} · {}", t("Headroom"), gain, t(application))
    };
    frame.render_widget(
        Paragraph::new(crate::ui::tui::studio::fit(&status, regions.status.width))
            .alignment(Alignment::Right)
            .style(Style::default().fg(
                if draft_pending || application == "Pending audio update" {
                    p.warning
                } else {
                    p.muted
                },
            )),
        regions.status,
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(p.edge)),
        area,
    );
}

fn draw_actions(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    for (rect, destination) in crate::ui::tui::studio::controls::actions(area)
        .into_iter()
        .zip(Workspace::ALL)
    {
        let key = match destination.shortcut() {
            crossterm::event::KeyCode::Char(ch) => ch.to_ascii_uppercase().to_string(),
            _ => "Esc".into(),
        };
        let selected = destination == view.workspace;
        frame.render_widget(
            Paragraph::new(crate::ui::tui::studio::fit(
                &format!("{key} {}", t(destination.action_label())),
                rect.width,
            ))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if selected {
                        view.palette.accent
                    } else {
                        view.palette.muted
                    })
                    .bg(if selected {
                        view.palette.selected
                    } else {
                        view.palette.bg
                    })
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            rect,
        );
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let pending = view.configuration.is_some_and(|editor| editor.pending());
    let contextual = match view.workspace {
        Workspace::Now => "Click / ↑↓ select · ←→ adjust",
        Workspace::Sound => "Arrows select · +/− draft · Enter apply · Esc cancel",
        Workspace::Apps if super::actions::mixer_mode(view.runtime) => {
            "←/→ volume · Space mute · X solo · [/] balance · U undo"
        }
        Workspace::Apps => "↑↓ choose · Space mark · A mark all · Enter apply",
        Workspace::Device => "O output",
        Workspace::Intelligence => "Choose a goal and preview",
        Workspace::System => "Callback budget is wall time, not CPU usage",
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(t(contextual), Style::default().fg(view.palette.text)),
                Span::raw("  ·  "),
                Span::styled(
                    t(if pending { "S stop" } else { "O output" }),
                    Style::default().fg(view.palette.secondary),
                ),
                Span::raw(format!(
                    " · {}",
                    t(if pending {
                        "? help"
                    } else {
                        "P preset · J AI · U undo · ? help · Q close"
                    })
                )),
            ]),
            Line::from(Span::styled(
                crate::ui::theme::clean(view.notice),
                Style::default().fg(view.palette.muted),
            )),
        ])
        // Keep the notice on its own row; shortcuts must never push errors or the
        // offline-fixture disclaimer out of the viewport.
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(view.palette.edge)),
        ),
        area,
    );
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    if view.overlay != Overlay::None && (area.width < 30 || area.height < 8) {
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(t("Enlarge terminal to confirm. Esc cancels.")),
            area,
        );
        return;
    }
    if matches!(view.overlay, Overlay::Preset | Overlay::Output)
        && view.runtime["selection_invalidated"] == true
    {
        modal(
            frame,
            area,
            "Selection details",
            vec![
                Line::from(t("Configuration changed; reopen the picker")),
                Line::from(t("Esc cancels")),
            ],
            view.palette,
        );
        return;
    }
    match view.overlay {
        Overlay::None => {}
        Overlay::Help => modal(
            frame,
            area,
            "HELP · simple mode",
            vec![
                Line::from(t(
                    "E EQ · M Apps · V Device · I Assist · H Health · Esc Studio",
                )),
                Line::from(t(
                    "Click a sound row to select; click its −/+ to adjust. Wheel only selects.",
                )),
                Line::from(t(
                    "Click O output or P preset in the header to open a picker; Enter confirms.",
                )),
                Line::from(t(
                    "EQ: click a band, then gain/Q. Apps: mark a scope, then apply.",
                )),
                Line::from(t(
                    "Mouse controls are inactive behind dialogs and in compact mode.",
                )),
                Line::from(t(
                    "O opens output picker; it never switches just by opening it",
                )),
                Line::from(t("P preset · J AI preview · U undo · S stop · L language")),
                Line::from(t(
                    "Settings: arrows browse; +/− stages; Enter applies; Esc cancels.",
                )),
                Line::from(t(
                    "APPS: Space marks apps, A selects all system audio, Enter applies once",
                )),
                Line::from(t(
                    "Enter confirms · Esc cancels · Q quits from the main screen",
                )),
            ],
            view.palette,
        ),
        Overlay::Output => draw_output_picker(frame, area, view),
        Overlay::Preset => draw_preset_picker(frame, area, view),
        Overlay::Proposal => draw_proposal(frame, area, view),
        Overlay::StartSystem => modal(
            frame,
            area,
            "START SYSTEM AUDIO",
            vec![
                Line::from(t(
                    "Maris will process system playback through its native audio tap.",
                )),
                Line::from(t(
                    "It does not change system volume or the macOS default output.",
                )),
                Line::from(t("Enter starts · Esc cancels")),
            ],
            view.palette,
        ),
    }
}

fn draw_output_picker(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let outputs = view
        .devices
        .iter()
        .filter(|device| device.direction == "output")
        .collect::<Vec<_>>();
    let mut rows = vec![Row::new([
        if view.output_choice == 0 { "▶" } else { " " }.to_owned(),
        t("Follow system default").into(),
        t("Automatic").into(),
    ])];
    rows.extend(outputs.iter().enumerate().map(|(index, device)| {
        let selected = view.output_choice == index + 1;
        Row::new([
            if selected { "▶" } else { " " }.to_owned(),
            crate::ui::theme::clean(&device.name),
            if device.is_default {
                t("System default").into()
            } else {
                t("Physical output").into()
            },
        ])
        .style(if selected {
            Style::default()
                .fg(view.palette.accent)
                .bg(view.palette.selected)
        } else {
            Style::default().fg(view.palette.text)
        })
    }));
    let areas =
        super::output_picker::layout(area, outputs.len().saturating_add(1), view.output_choice);
    let rect = areas.modal;
    frame.render_widget(Clear, rect);
    let mut state = TableState::default()
        .with_selected(Some(view.output_choice))
        .with_offset(areas.offset);
    frame.render_stateful_widget(
        Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Percentage(62),
                Constraint::Percentage(38),
            ],
        )
        .header(
            Row::new(["", t("OUTPUT"), t("MODE")]).style(
                Style::default()
                    .fg(view.palette.text)
                    .bg(view.palette.selected)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .row_highlight_style(
            Style::default()
                .fg(view.palette.accent)
                .bg(view.palette.selected)
                .add_modifier(Modifier::BOLD),
        )
        .block(modal_block(
            "OUTPUT · Enter switches · Esc cancels",
            view.palette,
        )),
        rect,
        &mut state,
    );
}

fn draw_preset_picker(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let areas = super::preset_picker::layout(area, view.presets.len(), view.preset_choice);
    let rect = areas.modal;
    let zones = [areas.table, areas.preview];
    frame.render_widget(Clear, rect);
    let block = modal_block("PRESETS · Enter applies · Esc cancels", view.palette);
    frame.render_widget(block, rect);
    let rows = view.presets.iter().enumerate().map(|(index, preset)| {
        Row::new([
            if index == view.preset_choice {
                "▶"
            } else {
                " "
            }
            .to_owned(),
            crate::i18n::preset_name(&preset.id, &preset.name).to_owned(),
            t(match preset.category {
                "scene" => "Scene",
                "listening" => "Listening preset",
                _ => "Tone curve",
            })
            .to_owned(),
            if preset.category == "eqmac" {
                "eqMac"
            } else {
                "Maris"
            }
            .to_owned(),
        ])
        .style(Style::default().fg(if index == view.preset_choice {
            view.palette.accent
        } else {
            view.palette.text
        }))
    });
    let mut state = TableState::default()
        .with_selected(Some(view.preset_choice))
        .with_offset(areas.offset);
    frame.render_stateful_widget(
        Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Percentage(50),
                Constraint::Percentage(30),
                Constraint::Percentage(20),
            ],
        )
        .header(
            Row::new(["", t("PRESET"), t("GROUP"), t("SOURCE")])
                .style(Style::default().fg(view.palette.muted)),
        ),
        zones[0],
        &mut state,
    );
    if let Some(preset) = view.presets.get(view.preset_choice) {
        let mut lines = vec![Line::from(t(&preset.description).to_owned())];
        if matches!(preset.category, "scene" | "listening") {
            let capability = serde_json::from_value(view.runtime["device_capability"].clone())
                .unwrap_or_default();
            let id = preset.id.strip_prefix("listening:").unwrap_or(&preset.id);
            if let Ok(preview) = crate::presets::scenes::prepare(view.music, &capability, id) {
                lines.push(Line::from(t("Correction stays unchanged")));
                if preview.compression_enabled {
                    lines.push(Line::from(t("Compression enabled")));
                }
                if preview.device_constraints_applied {
                    lines.push(Line::from(t("Device limits applied")));
                }
                lines.extend(
                    preview
                        .changes
                        .iter()
                        .take(3)
                        .map(|change| Line::from(crate::i18n::change(change))),
                );
            }
        } else {
            lines.clear();
            lines.push(Line::from(format!("{} · {}", t("Tone curve"), preset.id)));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(view.palette.muted)),
            zones[1],
        );
    }
}

fn draw_proposal(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let Some(proposal) = view.proposal else {
        return;
    };
    let mut lines = vec![
        Line::from(crate::i18n::format(
            "Goal: {goal} · Changes: {count}",
            &[
                ("goal", crate::i18n::label(&proposal.goal)),
                ("count", &proposal.changes.len().to_string()),
            ],
        )),
        Line::from(""),
    ];
    lines.extend(
        proposal
            .changes
            .iter()
            .take(8)
            .map(|change| Line::from(crate::i18n::change(change))),
    );
    lines.push(Line::from(""));
    lines.extend(
        proposal
            .reasons
            .iter()
            .take(3)
            .map(|reason| Line::from(crate::i18n::reason(reason))),
    );
    lines.push(Line::from(t("Enter applies · Esc cancels")));
    modal(frame, area, "AI PREVIEW", lines, view.palette);
}

fn modal(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'_>>, palette: Palette) {
    let rect = popup(area, &lines);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(modal_block(title, palette)),
        rect,
    );
}

fn modal_block(title: &str, palette: Palette) -> Block<'static> {
    Block::default()
        .title(
            Line::from(format!(" {} ", t(title))).style(
                Style::default()
                    .fg(palette.text)
                    .bg(palette.selected)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .title_bottom(format!(" {} ", t("Enter confirms · Esc cancels")))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette.accent))
        .style(Style::default().bg(palette.panel).fg(palette.text))
}
