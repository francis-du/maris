//! Built-in preset catalog browser shared by the TUI's preset page.
use crate::{
    i18n::text as t,
    presets::PresetSummary,
    ui::theme::{ACCENT, EDGE, MUTED, PANEL, TEXT},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {} ", t(title)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(EDGE))
        .style(Style::default().bg(PANEL).fg(TEXT))
}

pub fn filtered<'a>(catalog: &'a [PresetSummary], category: &str) -> Vec<&'a PresetSummary> {
    catalog
        .iter()
        .filter(|preset| category == "all" || preset.category == category)
        .collect()
}

pub fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    catalog: &[PresetSummary],
    category: &str,
    selected: usize,
    current: &str,
    rate: u32,
) {
    let zones = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(7),
    ])
    .spacing(1)
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "{}: {}   {}: {}   {} Hz   {}",
            t("Category"),
            if category == "all" {
                t("All")
            } else {
                category
            },
            t("Current preset"),
            crate::i18n::preset_name(current, current),
            rate,
            t("←/→ category · ↑/↓ select · Enter apply · B compare · U restore · P close")
        ))
        .block(panel("Preset catalog")),
        zones[0],
    );
    let filtered = filtered(catalog, category);
    let rows = filtered.iter().enumerate().map(|(index, preset)| {
        Row::new(vec![
            Cell::from(if index == selected { "▶" } else { " " }),
            Cell::from(preset.id.clone()),
            Cell::from(crate::i18n::preset_name(&preset.id, &preset.name).to_owned()),
            Cell::from(preset.source),
        ])
        .style(if index == selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        })
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(25),
                Constraint::Min(18),
                Constraint::Length(26),
            ],
        )
        .header(Row::new(["", t("ID"), t("Name"), t("Source")]).style(Style::default().fg(MUTED)))
        .block(panel("Presets")),
        zones[1],
    );
    let detail = filtered
        .get(selected)
        .and_then(|preset| crate::presets::show(&preset.id, rate).ok());
    let lines = if let Some(detail) = detail {
        vec![
            Line::from(format!("{}: {}", t("Stable ID"), detail.id)),
            Line::from(format!(
                "{}: {:.2} dB",
                t("Safe preamp"),
                detail.safe_preamp_db
            )),
            Line::from(format!(
                "{}: {}",
                t("Source"),
                detail.source_commit.map_or_else(
                    || detail.source.to_owned(),
                    |commit| format!("{} @ {}", detail.source, &commit[..8])
                )
            )),
            Line::from(format!(
                "{}: {}",
                t("Bandwidth"),
                detail.source_bandwidth_octaves.map_or_else(
                    || "Q".into(),
                    |bandwidth| format!("{bandwidth:.2} {}", t("Octave"))
                )
            )),
        ]
    } else {
        vec![Line::from(t("No preset available"))]
    };
    frame.render_widget(
        Paragraph::new(lines).block(panel("Preset details")),
        zones[2],
    );
}
