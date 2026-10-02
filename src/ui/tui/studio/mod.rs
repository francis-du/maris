//! Audio-workstation composition: one analyzer, a fixed control rail and quiet support areas.
//! UI-only rendering. See docs/development/ui-design.md for references and evidence boundaries.

pub mod controls;

use crate::{
    i18n::text as t,
    ui::theme::{self, Palette},
    ui::tui::dashboard,
    ui::tui::input as control_panel,
    ui::tui::view::Console,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as Stroke},
        Block, Borders, Paragraph,
    },
    Frame,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct StudioLayout {
    pub rail: Rect,
    pub analyzer: Rect,
    pub context: Rect,
    pub intelligence: Rect,
    pub mixer: Rect,
    pub health: Rect,
}

pub fn layout(area: Rect) -> StudioLayout {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(3)])
        .spacing(1)
        .split(area);
    let rail_width = (area.width.saturating_add(4) / 4)
        .clamp(28, 35)
        .min(area.width);
    let columns = Layout::horizontal([Constraint::Length(rail_width), Constraint::Min(1)])
        .spacing(2)
        .split(rows[0]);
    let support_height = if rows[0].height >= 24 { 7 } else { 5 };
    let work = Layout::vertical([Constraint::Min(1), Constraint::Length(support_height)])
        .spacing(1)
        .split(columns[1]);
    let support = Layout::horizontal([Constraint::Percentage(53), Constraint::Percentage(47)])
        .spacing(2)
        .split(work[1]);
    let status = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
        .spacing(2)
        .split(rows[1]);
    StudioLayout {
        rail: columns[0],
        analyzer: work[0],
        context: support[0],
        intelligence: support[1],
        mixer: status[0],
        health: status[1],
    }
}

fn row(area: Rect, index: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(index.min(area.height)),
        area.width,
        u16::from(index < area.height),
    )
}

fn inset(area: Rect, horizontal: u16) -> Rect {
    let pad = horizontal.min(area.width / 2);
    Rect::new(
        area.x + pad,
        area.y,
        area.width.saturating_sub(pad * 2),
        area.height,
    )
}

/// Ellipsize by terminal-cell width, not UTF-8 byte count.
pub fn fit(value: &str, width: u16) -> String {
    let value = theme::clean(value);
    if Span::raw(value.as_str()).width() <= usize::from(width) {
        return value;
    }
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut result = String::new();
    for ch in value.chars() {
        let cell_width = Span::raw(ch.to_string()).width();
        if used + cell_width >= usize::from(width) {
            break;
        }
        used += cell_width;
        result.push(ch);
    }
    result.push('…');
    result
}

fn write(frame: &mut Frame<'_>, area: Rect, value: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(fit(value, area.width)).style(style),
        row(area, 0),
    );
}

fn heading(frame: &mut Frame<'_>, area: Rect, title: &str, palette: Palette) {
    write(
        frame,
        area,
        t(title),
        Style::default()
            .fg(palette.muted)
            .add_modifier(Modifier::BOLD),
    );
}

pub fn live(runtime: &Value, now: u64) -> bool {
    runtime["active"] == true
        && runtime["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| now.checked_sub(time).is_some_and(|age| age < 1000))
}

fn number(value: Option<f64>, suffix: &str, digits: usize) -> String {
    value.filter(|v| v.is_finite()).map_or_else(
        || t("Unavailable").into(),
        |v| format!("{v:.digits$}{suffix}"),
    )
}

fn field(frame: &mut Frame<'_>, area: Rect, name: &str, value: &str, palette: Palette) {
    let label_width = (Span::raw(t(name)).width() as u16)
        .clamp(8, 20)
        .min(area.width.saturating_sub(12));
    let parts = Layout::horizontal([Constraint::Length(label_width), Constraint::Min(0)])
        .spacing(1)
        .split(area);
    write(frame, parts[0], t(name), Style::default().fg(palette.muted));
    write(frame, parts[1], value, Style::default().fg(palette.text));
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let regions = layout(area);
    let now = crate::analysis::now_ms();
    let is_live = live(view.runtime, now);
    draw_rail(frame, regions.rail, view);
    draw_analyzer(frame, regions.analyzer, view, is_live, now);
    draw_context(frame, regions.context, view, is_live, now);
    draw_intelligence(frame, regions.intelligence, view);
    draw_status(frame, regions.mixer, regions.health, view, is_live);
}

pub(crate) fn draw_rail(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    frame.render_widget(Block::default().style(Style::default().bg(p.panel)), area);
    let rail = crate::ui::tui::studio::controls::rail(area, view.sound_row);
    let device_height = rail.device.height;
    let regions = [rail.device, rail.controls];
    heading(frame, row(regions[0], 0), "Device", p);
    let capability = &view.runtime["device_capability"];
    let class = capability["device_class"]
        .as_str()
        .map(crate::i18n::label)
        .unwrap_or(t("Unknown"));
    write(
        frame,
        row(regions[0], 1),
        class,
        Style::default().fg(p.text),
    );
    field(
        frame,
        row(regions[0], 2),
        "AutoEq",
        capability["model"].as_str().unwrap_or(t("Unknown")),
        p,
    );
    let correction = view
        .music
        .correction_source
        .as_deref()
        .unwrap_or(t("No correction"));
    field(frame, row(regions[0], 3), "Correction", correction, p);
    if device_height >= 5 {
        let limits = match (
            capability["bass_floor_hz"].as_f64(),
            capability["treble_ceiling_hz"].as_f64(),
        ) {
            (Some(low), Some(high)) => format!("{low:.0}–{high:.0} Hz"),
            (Some(low), None) => format!("{} {low:.0} Hz", t("LF")),
            (None, Some(high)) => format!("{} {high:.0} Hz", t("HF")),
            _ => t("Unknown").into(),
        };
        field(frame, row(regions[0], 4), "Limits", &limits, p);
    }
    if device_height >= 6 {
        let route = match view.runtime["output_mode"].as_str() {
            Some("follow_system_default") => t("Follow system default"),
            Some("pinned") => t("Pinned output"),
            _ => t("Unavailable"),
        };
        write(
            frame,
            row(regions[0], 5),
            route,
            Style::default().fg(p.muted),
        );
    }

    let controls = regions[1];
    heading(frame, row(controls, 0), "Sound", p);
    let stride = rail.stride;
    let visible = rail.count;
    if visible == 0 {
        return;
    }
    let selected = view.sound_row.min(control_panel::DASHBOARD_SOUND_ROWS - 1);
    let start = rail.first;
    let values = dashboard::sound_rows(view);
    for (offset, index) in (start..(start + visible)).enumerate() {
        let rect = row(controls, 2 + offset as u16 * stride);
        let active = index == selected;
        if active {
            frame.render_widget(
                Block::default().style(Style::default().bg(p.selected)),
                rect,
            );
        }
        let parts = crate::ui::tui::studio::controls::parameter(rect);
        write(
            frame,
            parts.decrement,
            if active { "−" } else { " " },
            Style::default().fg(p.accent),
        );
        write(
            frame,
            parts.increment,
            if active { " +" } else { "  " },
            Style::default().fg(p.accent),
        );
        write(
            frame,
            parts.pointer,
            if active { "▶" } else { " " },
            Style::default().fg(p.accent),
        );
        write(
            frame,
            parts.label,
            &values[index].0,
            Style::default().fg(if active { p.text } else { p.muted }),
        );
        frame.render_widget(
            Paragraph::new(fit(&values[index].1, parts.value.width))
                .alignment(Alignment::Right)
                .style(
                    Style::default()
                        .fg(if active { p.accent } else { p.text })
                        .add_modifier(Modifier::BOLD),
                ),
            parts.value,
        );
        if stride == 2 {
            let track = inset(row(controls, 3 + offset as u16 * stride), 2);
            draw_track(frame, track, control_position(view, index), active, p);
        }
    }
}

fn control_position(view: &Console<'_>, index: usize) -> f64 {
    let music = view.music;
    match index {
        0 => (music.bass_db + 6.0) / 12.0,
        1 => {
            if music.bass_assist.enabled {
                music.bass_assist.amount
            } else {
                0.0
            }
        }
        2 => (music.presence_db + 3.0) / 6.0,
        3 => (music.air_db + 3.0) / 6.0,
        4 => music.softness,
        5 => music.intensity,
        6 => {
            if music.adaptive.enabled {
                music.adaptive.strength
            } else {
                0.0
            }
        }
        7 => music.width / 1.5,
        8 => (music.balance + 1.0) / 2.0,
        _ => f64::from(u8::from(music.compressor.enabled)),
    }
    .clamp(0.0, 1.0)
}

fn draw_track(frame: &mut Frame<'_>, area: Rect, amount: f64, selected: bool, p: Palette) {
    if area.width == 0 {
        return;
    }
    let point = (amount * f64::from(area.width.saturating_sub(1))).round() as usize;
    let spans = (0..usize::from(area.width))
        .map(|index| {
            Span::styled(
                if index == point { "●" } else { "─" },
                Style::default().fg(if index == point {
                    if selected {
                        p.accent
                    } else {
                        p.muted
                    }
                } else {
                    p.edge
                }),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(crate) fn draw_analyzer(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &Console<'_>,
    is_live: bool,
    now: u64,
) {
    if area.width < 24 || area.height < 6 {
        return;
    }
    let p = view.palette;
    frame.render_widget(Block::default().style(Style::default().bg(p.panel)), area);
    let inner = inset(area, 1);
    let columns = Layout::horizontal([Constraint::Min(1), Constraint::Length(16)])
        .spacing(1)
        .split(inner);
    let main = columns[0];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                t("Real-time spectrum"),
                Style::default()
                    .fg(p.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  ", Style::default().fg(p.muted)),
            Span::styled(t("Profile EQ"), Style::default().fg(p.accent)),
        ])),
        row(main, 0),
    );
    let compact_eq = view.workspace == control_panel::Workspace::Sound && main.height < 14;
    let plot = Rect::new(
        main.x + 4,
        main.y + 2,
        main.width.saturating_sub(9),
        main.height.saturating_sub(if compact_eq { 4 } else { 6 }),
    );
    let curve = crate::ui::tui::view::response(&view.snapshot.profile, view.sample_rate());
    let extent = curve
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .map(f64::abs)
        .fold(6.0, f64::max)
        .ceil();
    let fresh_spectrum = is_live
        && view.runtime["visualization"]["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| now.checked_sub(time).is_some_and(|age| age < 600));
    let bands = crate::ui::tui::monitor::bands(view.runtime);
    let has_signal = fresh_spectrum
        && bands
            .iter()
            .any(|value| value.is_finite() && *value > 0.001);
    let spectrum_color = theme::blend(p.panel, p.secondary, 0.32);
    frame.render_widget(
        Canvas::default()
            .background_color(p.panel)
            .marker(Marker::Braille)
            .x_bounds([0.0, 1.0])
            .y_bounds([0.0, 1.0])
            .paint(|ctx| {
                for hz in [20.0_f64, 100.0, 1000.0, 10000.0, 20000.0] {
                    let x = (hz / 20.0).log10() / 3.0;
                    ctx.draw(&Stroke {
                        x1: x,
                        y1: 0.0,
                        x2: x,
                        y2: 1.0,
                        color: p.edge,
                    });
                }
                for y in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    ctx.draw(&Stroke {
                        x1: 0.0,
                        y1: y,
                        x2: 1.0,
                        y2: y,
                        color: p.edge,
                    });
                }
                if has_signal {
                    let count = usize::from(plot.width).saturating_mul(2).max(2);
                    for index in 0..count {
                        let x = index as f64 / (count - 1) as f64;
                        let position = (x * 24.0 - 0.5).clamp(0.0, 23.0);
                        let a = position.floor() as usize;
                        let b = (a + 1).min(23);
                        let y =
                            (bands[a] + (bands[b] - bands[a]) * position.fract()).clamp(0.0, 1.0);
                        ctx.draw(&Stroke {
                            x1: x,
                            y1: 0.0,
                            x2: x,
                            y2: y,
                            color: spectrum_color,
                        });
                    }
                    ctx.layer();
                    for index in 1..24 {
                        ctx.draw(&Stroke {
                            x1: (index as f64 - 0.5) / 24.0,
                            y1: bands[index - 1].clamp(0.0, 1.0),
                            x2: (index as f64 + 0.5) / 24.0,
                            y2: bands[index].clamp(0.0, 1.0),
                            color: p.secondary,
                        });
                    }
                }
                ctx.layer();
                for (index, pair) in curve.windows(2).enumerate() {
                    ctx.draw(&Stroke {
                        x1: index as f64 / 179.0,
                        y1: 0.5 + pair[0] / (2.0 * extent),
                        x2: (index + 1) as f64 / 179.0,
                        y2: 0.5 + pair[1] / (2.0 * extent),
                        color: p.accent,
                    });
                }
                if view.workspace == control_panel::Workspace::Sound {
                    if let Some(band) = view
                        .sound_row
                        .checked_sub(control_panel::EQ_ROW_START)
                        .and_then(|index| view.snapshot.profile.bands.get(index))
                    {
                        let x = ((band.frequency_hz / 20.0).log10() / 3.0).clamp(0.0, 1.0);
                        let index = (x * 179.0).round() as usize;
                        let y = 0.5 + curve[index] / (2.0 * extent);
                        ctx.layer();
                        ctx.draw(&Stroke {
                            x1: x,
                            y1: 0.0,
                            x2: x,
                            y2: 1.0,
                            color: theme::blend(p.panel, p.accent, 0.35),
                        });
                        ctx.draw(&Stroke {
                            x1: (x - 0.01).max(0.0),
                            y1: y,
                            x2: (x + 0.01).min(1.0),
                            y2: y,
                            color: p.text,
                        });
                        ctx.draw(&Stroke {
                            x1: x,
                            y1: (y - 0.04).max(0.0),
                            x2: x,
                            y2: (y + 0.04).min(1.0),
                            color: p.text,
                        });
                    }
                }
            }),
        plot,
    );
    for (fraction, label) in [(0.0, "0"), (0.5, "-36"), (1.0, "-72")] {
        let y = plot.y + (fraction * f64::from(plot.height.saturating_sub(1))).round() as u16;
        write(
            frame,
            Rect::new(main.x, y, 4, 1),
            label,
            Style::default().fg(p.muted),
        );
    }
    for (fraction, label) in [
        (0.0, format!("+{extent:.0}")),
        (0.5, "0".into()),
        (1.0, format!("-{extent:.0}")),
    ] {
        let y = plot.y + (fraction * f64::from(plot.height.saturating_sub(1))).round() as u16;
        write(
            frame,
            Rect::new(plot.right() + 1, y, 4, 1),
            &label,
            Style::default().fg(p.accent),
        );
    }
    for (hz, label) in [
        (20.0_f64, "20"),
        (100.0, "100"),
        (1000.0, "1k"),
        (10000.0, "10k"),
        (20000.0, "20k"),
    ] {
        if plot.width < 60 && hz == 10000.0 {
            continue;
        }
        let offset =
            ((hz / 20.0).log10() / 3.0 * f64::from(plot.width.saturating_sub(1))).round() as u16;
        let x = (plot.x + offset).min(plot.right().saturating_sub(label.len() as u16));
        write(
            frame,
            Rect::new(x, plot.bottom(), label.len() as u16, 1),
            label,
            Style::default().fg(p.muted),
        );
    }
    if !has_signal {
        frame.render_widget(
            Paragraph::new(t("Waiting for signal"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(p.muted)),
            Rect::new(plot.x, plot.y + plot.height / 3, plot.width, 1),
        );
    }
    let scope = if main.width >= 68 {
        "Spectrum: pre-EQ dBFS  /  Profile EQ: calculated dB"
    } else {
        "pre-EQ dBFS  /  profile EQ dB"
    };
    write(
        frame,
        row(
            main,
            main.height.saturating_sub(if compact_eq { 1 } else { 3 }),
        ),
        t(scope),
        Style::default().fg(p.muted),
    );
    if compact_eq {
        draw_output_meters(frame, columns[1], view, is_live);
        return;
    }
    let analysis = &view.runtime["analysis"];
    let fresh_analysis = is_live
        && analysis["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| now.checked_sub(time).is_some_and(|age| age < 6000))
        && analysis["sample_rate"].as_u64() == view.runtime["sample_rate"].as_u64();
    let lufs = number(
        analysis["momentary_lufs"]
            .as_f64()
            .filter(|_| fresh_analysis),
        "",
        1,
    );
    let tp = number(
        analysis["true_peak_dbtp"]
            .as_f64()
            .filter(|_| fresh_analysis),
        "",
        1,
    );
    let mut metrics = format!("LUFS {lufs}   TP {tp} dBTP");
    if main.width >= 75 {
        metrics.push_str(&format!(
            "   RMS {} dBFS",
            number(
                analysis["rms_dbfs"].as_f64().filter(|_| fresh_analysis),
                "",
                1
            )
        ));
    }
    write(
        frame,
        row(main, main.height.saturating_sub(1)),
        &metrics,
        Style::default().fg(p.text),
    );
    let reduction = view.runtime["adaptive_reduction_db"]
        .as_f64()
        .filter(|v| is_live && v.is_finite() && *v >= 0.0 && view.music.adaptive.enabled);
    let label = format!(
        "{}: {}",
        t("Dynamic EQ"),
        number(reduction.map(|v| -v), " dB", 1)
    );
    write(
        frame,
        row(main, main.height.saturating_sub(2)),
        &label,
        Style::default().fg(if reduction.is_some_and(|v| v > 0.05) {
            p.warning
        } else {
            p.muted
        }),
    );
    draw_output_meters(frame, columns[1], view, is_live);
}

fn draw_output_meters(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>, is_live: bool) {
    let p = view.palette;
    heading(frame, row(area, 0), "Output", p);
    let value = |key: &str| {
        view.runtime[key]
            .as_f64()
            .filter(|value| value.is_finite() && is_live)
    };
    let left = value("peak_left_dbfs");
    let right = value("peak_right_dbfs");
    let height = area.height.saturating_sub(6);
    write(
        frame,
        row(area, 2),
        "     L     R",
        Style::default().fg(p.muted),
    );
    for (fraction, label) in [
        (0.0, "0"),
        (0.2, "-12"),
        (0.4, "-24"),
        (0.8, "-48"),
        (1.0, "-60"),
    ] {
        if height >= 8 {
            let y = area.y + 3 + (fraction * f64::from(height.saturating_sub(1))).round() as u16;
            write(
                frame,
                Rect::new(area.x, y, 3, 1),
                label,
                Style::default().fg(p.muted),
            );
        }
    }
    for (index, db) in [left, right].into_iter().enumerate() {
        let fraction = db.map_or(0.0, |db| ((db + 60.0) / 60.0).clamp(0.0, 1.0));
        let x = area.x + 5 + index as u16 * 6;
        for offset in 0..height {
            let on = fraction * f64::from(height) > f64::from(height - 1 - offset);
            let segment_db = -60.0 + 60.0 * f64::from(height - offset) / f64::from(height.max(1));
            let color = if !on {
                p.edge
            } else if segment_db > -3.0 && db.is_some_and(|db| db > -3.0) {
                theme::DANGER
            } else if segment_db >= -12.0 && db.is_some_and(|db| db >= -12.0) {
                p.warning
            } else {
                p.meter
            };
            write(
                frame,
                Rect::new(x, area.y + 3 + offset, 2, 1),
                "▐▌",
                Style::default().fg(color),
            );
        }
        let label = db.map_or_else(|| "—".into(), |db| format!("{db:.1}"));
        write(
            frame,
            Rect::new(
                area.x + 3 + index as u16 * 6,
                area.bottom().saturating_sub(2),
                6,
                1,
            ),
            &label,
            Style::default().fg(p.text),
        );
    }
    write(
        frame,
        row(area, area.height.saturating_sub(1)),
        if left.is_none() && right.is_none() {
            t("Unavailable")
        } else {
            "dBFS"
        },
        Style::default().fg(p.muted),
    );
}

fn current_context(
    view: &Console<'_>,
    is_live: bool,
    now: u64,
) -> Option<crate::analysis::context::MusicContext> {
    if !is_live {
        return None;
    }
    let evidence = crate::tuning::planner::preview_evidence(view.runtime, now).ok()?;
    let context = crate::tuning::planner::live_context(view.runtime, &evidence, now);
    (context.mode == "semantic").then_some(context)
}

fn draw_context(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>, is_live: bool, now: u64) {
    let p = view.palette;
    heading(frame, row(area, 0), "Music context", p);
    let context = current_context(view, is_live, now);
    if context.is_none() {
        let evidence = crate::tuning::planner::preview_evidence(view.runtime, now).ok();
        let bass = evidence
            .as_ref()
            .map(|a| a.band_energy_share[..3].iter().sum::<f64>() * 100.0);
        let treble = evidence
            .as_ref()
            .map(|a| a.band_energy_share[7..].iter().sum::<f64>() * 100.0);
        field(frame, row(area, 1), "Bass energy", &number(bass, "%", 0), p);
        field(
            frame,
            row(area, 2),
            "Treble energy",
            &number(treble, "%", 0),
            p,
        );
        field(
            frame,
            row(area, 3),
            "Theme source",
            if evidence.is_some() && view.palette.source == "signal" {
                t("Signal")
            } else {
                t("Unavailable")
            },
            p,
        );
        field(
            frame,
            row(area, 4),
            "Crest",
            &number(evidence.as_ref().map(|a| a.crest_db), " dB", 1),
            p,
        );
        if area.height >= 7 {
            field(
                frame,
                row(area, 5),
                "Stereo correlation",
                &number(evidence.as_ref().map(|a| a.stereo_correlation), "", 2),
                p,
            );
            write(
                frame,
                row(area, 6),
                t("Local signal-guided tuning"),
                Style::default().fg(p.muted),
            );
        }
        return;
    }
    let genre = context
        .as_ref()
        .and_then(|c| c.top_genre())
        .unwrap_or(t("Unknown"));
    field(frame, row(area, 1), "Genre", genre, p);
    let instruments = context
        .as_ref()
        .map(|c| {
            c.instruments
                .iter()
                .filter(|tag| tag.confidence >= 0.5)
                .take(3)
                .map(|tag| theme::clean(&tag.label))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| t("Unknown").into());
    field(frame, row(area, 2), "Instruments", &instruments, p);
    let source = match view.palette.source {
        "semantic" if context.is_some() => t("Semantic"),
        "signal" if is_live => t("Signal"),
        _ => t("Unavailable"),
    };
    field(frame, row(area, 3), "Theme source", source, p);
    field(
        frame,
        row(area, 4),
        "Vocal",
        &number(
            context
                .as_ref()
                .and_then(|c| c.vocal_probability)
                .map(|v| v * 100.0),
            "%",
            0,
        ),
        p,
    );
    if area.height >= 7 {
        field(
            frame,
            row(area, 5),
            "Confidence",
            &number(context.as_ref().map(|c| c.confidence * 100.0), "%", 0),
            p,
        );
        field(
            frame,
            row(area, 6),
            "Model",
            context
                .as_ref()
                .map_or(t("Unavailable"), |c| c.model.as_str()),
            p,
        );
    }
}

fn draw_intelligence(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    heading(frame, row(area, 0), "Listening Assist", p);
    field(
        frame,
        row(area, 1),
        "Goal",
        crate::i18n::label(view.goal),
        p,
    );
    if let Some(plan) = view.proposal {
        field(
            frame,
            row(area, 2),
            "Delta",
            &crate::i18n::format(
                "Changes: {count}",
                &[("count", &plan.changes.len().to_string())],
            ),
            p,
        );
        write(
            frame,
            row(area, 3),
            &plan
                .reasons
                .first()
                .map_or_else(|| t("Preview ready").into(), |s| crate::i18n::reason(s)),
            Style::default().fg(p.text),
        );
    } else {
        field(
            frame,
            row(area, 2),
            "Current plan",
            t("Preview required"),
            p,
        );
        write(
            frame,
            row(area, 3),
            t("J Preview   U Undo"),
            Style::default().fg(p.accent),
        );
    }
    if area.height >= 5 {
        let ready =
            crate::tuning::planner::preview_evidence(view.runtime, crate::analysis::now_ms())
                .is_ok();
        write(
            frame,
            row(area, 4),
            if ready {
                t("Ready for preview")
            } else {
                t("Waiting for signal")
            },
            Style::default().fg(p.muted),
        );
    }
    if area.height >= 7 {
        write(
            frame,
            row(area, 6),
            t("No model download needed"),
            Style::default().fg(p.muted),
        );
    }
}

pub(crate) fn draw_status(
    frame: &mut Frame<'_>,
    mixer: Rect,
    health: Rect,
    view: &Console<'_>,
    is_live: bool,
) {
    let p = view.palette;
    for (area, title) in [(mixer, "Mixer summary"), (health, "System health")] {
        frame.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(p.edge)),
            area,
        );
        heading(frame, row(area, 1), title, p);
    }
    let config = &view.runtime["mixer_control"]["config"];
    let strips = view.runtime["mixer_strip_count"]
        .as_u64()
        .map_or_else(|| t("Unavailable").into(), |count| count.to_string());
    let a = config["buses"][0]["output_device"].as_str().unwrap_or("—");
    let b = config["buses"][1]["output_device"].as_str().unwrap_or("—");
    write(
        frame,
        row(mixer, 2),
        &format!(
            "{strips} {} · A {} · B {} · {}",
            t("Strips"),
            fit(a, 14),
            fit(b, 14),
            t("Configuration only")
        ),
        Style::default().fg(p.muted),
    );
    let xruns = view.runtime["underruns"]
        .as_u64()
        .zip(view.runtime["overruns"].as_u64())
        .filter(|_| is_live)
        .map_or_else(
            || t("Unavailable").into(),
            |(a, b)| a.saturating_add(b).to_string(),
        );
    let revision = format!("r{}", view.snapshot.revision);
    let revision_area = Rect::new(
        health.right().saturating_sub(8),
        health.y + 1,
        8.min(health.width),
        1,
    );
    frame.render_widget(
        Paragraph::new(revision)
            .alignment(Alignment::Right)
            .style(Style::default().fg(p.muted)),
        revision_area,
    );
    let latency = view.runtime["reported_output_path_latency_ms"]
        .as_f64()
        .filter(|value| value.is_finite() && is_live)
        .map_or_else(
            || format!("{} {}", t("Latency"), t("Unavailable")),
            |value| format!("{value:.1} ms {}", t("Reported")),
        );
    write(
        frame,
        row(health, 2),
        &format!("{} {xruns} · {latency}", t("Xruns")),
        Style::default().fg(p.muted),
    );
}
