//! Compact monitor shared by small terminals and the main console. No synthetic animation.
use crate::{i18n::text as t, ui::theme::*};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use serde_json::Value;

pub fn bands(runtime: &Value) -> [f64; 24] {
    std::array::from_fn(|i| {
        runtime["display_bands"][i]
            .as_f64()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    })
}
pub fn spectrum(frame: &mut Frame<'_>, area: Rect, values: &[f64; 24]) {
    spectrum_colored(frame, area, values, VIOLET);
}

pub fn spectrum_colored(frame: &mut Frame<'_>, area: Rect, values: &[f64; 24], palette: Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let count = (area.width as usize / 2).clamp(1, 24);
    let gap = if area.width as usize > count { 1 } else { 0 };
    let symbols = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let lines: Vec<Line<'static>> = (0..area.height)
        .map(|row| {
            let spans = (0..count)
                .flat_map(|i| {
                    let start = i * 24 / count;
                    let end = ((i + 1) * 24 / count).max(start + 1);
                    let value = values[start..end].iter().copied().fold(0.0, f64::max);
                    let partial = (value * area.height as f64 - (area.height - row - 1) as f64)
                        .clamp(0.0, 1.0);
                    let glyph = symbols[(partial * 8.0).round() as usize];
                    let hue = if count <= 1 {
                        0.0
                    } else {
                        i as f64 / (count - 1) as f64
                    };
                    let color = blend(palette.accent, palette.secondary, hue);
                    [
                        Span::styled(
                            glyph,
                            Style::default().fg(if partial > 0.0 { color } else { palette.edge }),
                        ),
                        Span::raw(" ".repeat(gap)),
                    ]
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
pub fn compact(frame: &mut Frame<'_>, area: Rect, runtime: &Value, bypass: bool) {
    compact_colored(frame, area, runtime, bypass, palette(runtime));
}

pub fn compact_colored(
    frame: &mut Frame<'_>,
    area: Rect,
    runtime: &Value,
    bypass: bool,
    palette: Palette,
) {
    let width = area.width.min(54);
    let height = area.height.min(9);
    let area = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let block = Block::default()
        .title(" MARIS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette.accent))
        .style(Style::default().bg(palette.panel).fg(palette.text));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let now = crate::analysis::now_ms();
    let is_live = crate::ui::tui::studio::live(runtime, now);
    let spectrum_fresh = is_live
        && runtime["visualization"]["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| now.checked_sub(time).is_some_and(|age| age < 600));
    let reduced_motion = super::studio::appearance::Appearance::from_environment().reduced_motion;
    let mode = t(if runtime["active"] == true && !is_live {
        "STALE"
    } else if !is_live {
        "STANDBY"
    } else if bypass {
        "EQ BYPASS"
    } else {
        "LIVE"
    });
    let device = clean(runtime["output"].as_str().unwrap_or(t("System default")));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {mode}  "),
                Style::default()
                    .fg(if is_live {
                        palette.meter
                    } else if runtime["active"] == true {
                        palette.warning
                    } else {
                        palette.muted
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(device),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    if inner.height > 3 && !reduced_motion {
        spectrum_colored(
            frame,
            Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 3),
            &if spectrum_fresh {
                bands(runtime)
            } else {
                [0.0; 24]
            },
            palette,
        );
    }
    if inner.height > 3 && reduced_motion {
        frame.render_widget(
            Paragraph::new(t("Reduced motion"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(palette.muted)),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
    if inner.height > 1 {
        let peak = runtime["peak_dbfs"]
            .as_f64()
            .filter(|v| v.is_finite() && is_live)
            .map_or_else(|| t("Unavailable").to_owned(), |v| format!("{v:.1} dBFS"));
        let text = if is_live {
            format!("{peak}   S · {}   Q · {}", t("Stop"), t("Close"))
        } else {
            format!("{}   Q · {}", t("Resize to expand"), t("Close"))
        };
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .style(Style::default().fg(palette.muted)),
            Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
        );
    }
}
