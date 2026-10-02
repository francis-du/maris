//! Listening-focused panel: every adjustment targets the current device profile.
use crate::ui::theme::*;
use crate::{
    control::store::Store, dsp::music::MusicProfile, i18n::text as t,
    tuning::preferences as listening,
};
use anyhow::{ensure, Result};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};
pub fn draw(
    frame: &mut Frame<'_>,
    profile: &MusicProfile,
    device: &str,
    selected: usize,
    notice: &str,
) {
    draw_live(
        frame,
        profile,
        device,
        selected,
        notice,
        &serde_json::json!({}),
    );
}
pub fn draw_live(
    frame: &mut Frame<'_>,
    profile: &MusicProfile,
    device: &str,
    selected: usize,
    notice: &str,
    runtime: &serde_json::Value,
) {
    frame.render_widget(
        Block::default().style(Style::default().bg(BG).fg(TEXT)),
        frame.area(),
    );
    let zones = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(11),
        Constraint::Length(5),
    ])
    .margin(1)
    .split(frame.area());
    let block = |title: String| {
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(EDGE))
            .style(Style::default().bg(PANEL).fg(TEXT))
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("MARIS · {}", t("MUSIC ENHANCEMENT"))),
            Line::from(format!(
                "{} · {}",
                device,
                t(if profile.reference {
                    "Reference A"
                } else {
                    "Enhanced B"
                })
            )),
        ])
        .block(block(t("MUSIC ENHANCEMENT").into())),
        ratatui::layout::Rect::new(zones[0].x, zones[0].y, zones[0].width, 4),
    );
    crate::ui::tui::monitor::spectrum(
        frame,
        ratatui::layout::Rect::new(zones[0].x, zones[0].y + 4, zones[0].width, 4),
        &crate::ui::tui::monitor::bands(runtime),
    );
    let values = [
        ("Bass", format!("{:+.1} dB", profile.bass_db)),
        (
            "Bass Assist",
            if profile.bass_assist.enabled {
                format!(
                    "{} · {:.0}%",
                    t("Virtual bass"),
                    profile.bass_assist.amount * 100.0
                )
            } else {
                t("OFF").into()
            },
        ),
        ("Presence", format!("{:+.1} dB", profile.presence_db)),
        ("Air", format!("{:+.1} dB", profile.air_db)),
        ("Softness", format!("{:.0}%", profile.softness * 100.0)),
        ("Intensity", format!("{:.0}%", profile.intensity * 100.0)),
        (
            "Adaptive EQ",
            format!(
                "{:.0}% · -{:.1} dB",
                if profile.adaptive.enabled {
                    profile.adaptive.strength * 100.0
                } else {
                    0.0
                },
                runtime["adaptive_reduction_db"].as_f64().unwrap_or(0.0)
            ),
        ),
        ("Stereo width", format!("{:.0}%", profile.width * 100.0)),
        ("Balance", format!("{:+.2}", profile.balance)),
        (
            "Compressor",
            if profile.compressor.enabled {
                t("ON").into()
            } else {
                t("OFF").into()
            },
        ),
    ];
    let rows = values.iter().enumerate().map(|(i, (name, value))| {
        Row::new([
            format!("{} {}", if i == selected { "▶" } else { " " }, t(name)),
            value.clone(),
        ])
        .style(if i == selected {
            Style::default()
                .fg(ACCENT)
                .bg(SELECTED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        })
    });
    frame.render_widget(
        Table::new(
            rows,
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .block(block(
            t("Up/Down select · Left/Right adjust · B compare · K save · Esc back").into(),
        )),
        zones[1],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(
                profile
                    .correction_source
                    .as_deref()
                    .unwrap_or(t("No measured correction")),
            ),
            Line::from(format!(
                "1–6 {} · {}",
                t("Presets"),
                t("E enhancement · B compare · K save device · L language")
            )),
            Line::from(if runtime["tonal_bypass"] == true {
                t("All effects bypassed · Space to enable")
            } else if profile.reference {
                t("Reference A")
            } else {
                notice
            }),
        ])
        .wrap(Wrap { trim: true }),
        zones[2],
    );
}
pub fn adjust(
    store: &Store,
    device: Option<&str>,
    revision: u64,
    selected: usize,
    direction: f64,
) -> Result<bool> {
    ensure!(selected < 10, "Unknown sound control");
    ensure!(
        direction == -1.0 || direction == 1.0,
        "Adjustment direction must be -1 or 1"
    );
    let library = listening::load(store)?;
    ensure!(
        library.revision == revision,
        "Listening revision conflict: expected {revision}, current {}",
        library.revision
    );
    let before = device.map_or(&library.default, |key| library.effective(key));
    let capability = device
        .map(|key| crate::devices::capability::effective(store, key))
        .transpose()?
        .unwrap_or_default();
    let target = adjusted_profile(before, &capability, selected, direction)?;
    let applied = listening::edit_if_changed(store, Some(revision), device, |profile| {
        ensure!(
            *profile == *before,
            "Listening profile changed after preview"
        );
        *profile = target;
        Ok(())
    })?;
    Ok(applied.revision != revision)
}

/// Pure counterpart shared by direct controls and the configuration draft editor.
pub fn adjusted_profile(
    before: &MusicProfile,
    capability: &crate::devices::capability::Capability,
    selected: usize,
    direction: f64,
) -> Result<MusicProfile> {
    ensure!(selected < 10, "Unknown sound control");
    ensure!(
        direction == -1.0 || direction == 1.0,
        "Adjustment direction must be -1 or 1"
    );
    before.validate()?;
    capability.validate()?;
    let mut target = before.clone();
    let gain_step = |value: f64, lower: f64, upper: f64| {
        if direction > 0.0 && value >= upper {
            // Preserve legacy stored preferences instead of making '+' lower a
            // value, enable processing, or save an inaudible capped adjustment.
            value
        } else {
            (value.min(upper) + direction * 0.5).clamp(lower, upper)
        }
    };
    match selected {
        0 => target.bass_db = gain_step(before.bass_db, -6.0, capability.max_preference_boost_db),
        1 => {
            // OFF is the visible zero position, not the hidden remembered amount.
            if !before.bass_assist.enabled && direction < 0.0 {
                return Ok(before.clone());
            }
            if direction > 0.0 {
                ensure!(
                    capability.virtual_bass_allowed,
                    "Bass Assist is unavailable for this output"
                );
            }
            let amount = if before.bass_assist.enabled {
                before.bass_assist.amount
            } else {
                0.0
            };
            target.bass_assist.amount = (amount + direction * 0.1).clamp(0.0, 1.0);
            target.bass_assist.enabled = target.bass_assist.amount > 0.001;
        }
        2 => {
            target.presence_db = gain_step(
                before.presence_db,
                -3.0,
                capability.max_preference_boost_db.min(3.0),
            )
        }
        3 => {
            target.air_db = gain_step(
                before.air_db,
                -3.0,
                capability.max_preference_boost_db.min(3.0),
            )
        }
        4 => target.softness = (before.softness + direction * 0.1).clamp(0.0, 1.0),
        5 => target.intensity = (before.intensity + direction * 0.1).clamp(0.0, 1.0),
        6 => {
            if !before.adaptive.enabled && direction < 0.0 {
                return Ok(before.clone());
            }
            let strength = if before.adaptive.enabled {
                before.adaptive.strength
            } else {
                0.0
            };
            target.adaptive.strength = (strength + direction * 0.1).clamp(0.0, 1.0);
            target.adaptive.enabled = target.adaptive.strength > 0.001;
        }
        7 => target.width = (before.width + direction * 0.05).clamp(0.0, 1.5),
        8 => target.balance = (before.balance + direction * 0.05).clamp(-1.0, 1.0),
        9 => target.compressor.enabled = direction > 0.0,
        _ => unreachable!("validated sound row"),
    }
    // A no-op must not activate processing, exit A/B, materialize a device entry or
    // consume history. Explicit SaveDevice remains a separate operation.
    if target == *before {
        return Ok(before.clone());
    }
    target.reference = false;
    target.enabled = true;
    target.validate()?;
    Ok(target)
}
