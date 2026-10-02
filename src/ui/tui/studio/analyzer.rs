//! Measured spectrum and calculated profile response share a labeled frequency axis.
//! Rendering remains independent of audio processing, routing and model inference.
use super::{appearance, heading, inset, number, row, write};
use crate::{
    i18n::text as t,
    ui::theme,
    ui::tui::{input as control_panel, view::Console},
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
    let reduced_motion = appearance::Appearance::from_environment().reduced_motion;
    frame.render_widget(Block::default().style(Style::default().bg(p.panel)), area);
    let inner = inset(area, 1);
    let columns = Layout::horizontal([Constraint::Min(1), Constraint::Length(16)])
        .spacing(1)
        .split(inner);
    let main = columns[0];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("▰ {}", t("Real-time spectrum")),
                Style::default()
                    .fg(p.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ", Style::default().fg(p.muted)),
            Span::styled(
                format!("━ {}", t("Profile EQ")),
                Style::default().fg(p.accent),
            ),
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
                if has_signal && !reduced_motion {
                    let count = usize::from(plot.width).saturating_mul(2).max(2);
                    for index in 0..count {
                        let x = index as f64 / (count - 1) as f64;
                        let position = (x * 24.0 - 0.5).clamp(0.0, 23.0);
                        let a = position.floor() as usize;
                        let b = (a + 1).min(23);
                        let y =
                            (bands[a] + (bands[b] - bands[a]) * position.fract()).clamp(0.0, 1.0);
                        for slice in 0..4 {
                            let start = y * slice as f64 / 4.0;
                            let end = y * (slice + 1) as f64 / 4.0;
                            ctx.draw(&Stroke {
                                x1: x,
                                y1: start,
                                x2: x,
                                y2: end,
                                color: theme::blend(
                                    p.panel,
                                    p.secondary,
                                    0.18 + slice as f64 * 0.09,
                                ),
                            });
                        }
                    }
                    ctx.layer();
                    for index in 1..24 {
                        ctx.draw(&Stroke {
                            x1: (index as f64 - 0.5) / 24.0,
                            y1: bands[index - 1].clamp(0.0, 1.0),
                            x2: (index as f64 + 0.5) / 24.0,
                            y2: bands[index].clamp(0.0, 1.0),
                            color: theme::blend(p.secondary, p.meter, index as f64 / 23.0 * 0.35),
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
    if !has_signal || reduced_motion {
        let state = if reduced_motion {
            "Reduced motion"
        } else if view.runtime["active"] == true && (!is_live || !fresh_spectrum) {
            "STALE"
        } else if !is_live {
            "STANDBY"
        } else {
            "Waiting for signal"
        };
        frame.render_widget(
            Paragraph::new(t(state))
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
    let reduced_motion = appearance::Appearance::from_environment().reduced_motion;
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(p.edge)),
        area,
    );
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
            if reduced_motion {
                continue;
            }
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
                "██",
                Style::default().fg(color),
            );
        }
        let label = db.map_or_else(|| "—".into(), |db| format!("{db:.1}"));
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Right)
                .style(Style::default().fg(if db.is_some_and(|db| db > -3.0) {
                    theme::DANGER
                } else {
                    p.text
                })),
            Rect::new(
                area.x + 3 + index as u16 * 6,
                area.bottom().saturating_sub(2),
                6,
                1,
            ),
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
