//! One configuration work area: fixed section selectors, read-only list, draft station and commit bar.
use super::{is_switch, Draft, Group};
use crate::{
    i18n::text as t,
    ui::tui::dashboard,
    ui::tui::input::{self as control_panel, Overlay, UndoTarget, Workspace, EQ_ROW_START},
    ui::tui::studio::controls::{self as studio_controls, PointerAction},
    ui::tui::studio::{self as studio_view, fit},
    ui::tui::view::Console,
};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

#[derive(Clone, Copy, Debug)]
pub struct Areas {
    pub browser: Rect,
    pub target: Rect,
    pub analyzer: Rect,
    pub station: Rect,
    pub review: Rect,
    pub apply: Rect,
    pub cancel: Rect,
    pub minus: Rect,
    pub plus: Rect,
    pub q_minus: Rect,
    pub q_plus: Rect,
}
pub fn layout(area: Rect) -> Areas {
    let columns = Layout::horizontal([
        Constraint::Length((area.width / 3).clamp(28, 36)),
        Constraint::Min(1),
    ])
    .spacing(2)
    .split(area);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(7),
        Constraint::Length(if area.height >= 28 { 7 } else { 3 }),
        Constraint::Length(3),
    ])
    .spacing(u16::from(area.height >= 28))
    .split(columns[1]);
    let buttons = Layout::horizontal([
        Constraint::Percentage(45),
        Constraint::Min(3),
        Constraint::Percentage(35),
    ])
    .split(rows[4]);
    let edit = Layout::horizontal([
        Constraint::Length(9),
        Constraint::Min(1),
        Constraint::Length(9),
    ])
    .spacing(2)
    .split(row(rows[2], 3, 3));
    let q = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(7),
        Constraint::Length(7),
    ])
    .spacing(2)
    .split(row(rows[2], 6, 1));
    Areas {
        browser: columns[0],
        target: rows[0],
        analyzer: rows[1],
        station: rows[2],
        review: rows[3],
        apply: buttons[0],
        cancel: buttons[2],
        minus: edit[0],
        plus: edit[2],
        q_minus: q[1],
        q_plus: q[2],
    }
}
fn row(area: Rect, offset: u16, height: u16) -> Rect {
    Rect::new(
        area.x,
        area.y + offset.min(area.height),
        area.width,
        height.min(area.height.saturating_sub(offset)),
    )
}
pub fn group_rects(browser: Rect) -> [Rect; 3] {
    std::array::from_fn(|index| row(browser, 2 + index as u16, 1))
}
/// Stable pages, not a recentered list. Selecting a visible row cannot move any target.
pub fn browser_rows(area: Rect, selected: usize) -> Vec<(usize, Rect)> {
    let list = row(area, 7, area.height.saturating_sub(8));
    let rows = Group::of(selected).rows();
    let position = rows.iter().position(|id| *id == selected).unwrap_or(0);
    let count = usize::from(list.height).max(1);
    let start = position / count * count;
    rows.iter()
        .enumerate()
        .skip(start)
        .take(usize::from(list.height))
        .map(|(index, id)| (*id, row(list, (index - start) as u16, 1)))
        .collect()
}
fn put(frame: &mut Frame<'_>, area: Rect, text: &str, style: Style) {
    if area.width > 0 && area.height > 0 {
        frame.render_widget(Paragraph::new(fit(text, area.width)).style(style), area);
    }
}
fn button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    view: &Console<'_>,
    enabled: bool,
    primary: bool,
) {
    let p = view.palette;
    frame.render_widget(
        Block::default().style(Style::default().bg(if enabled && primary {
            p.accent
        } else {
            p.selected
        })),
        area,
    );
    frame.render_widget(
        Paragraph::new(fit(label, area.width))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if enabled && primary {
                        p.bg
                    } else if enabled {
                        p.text
                    } else {
                        p.muted
                    })
                    .add_modifier(if enabled {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        row(area, area.height.saturating_sub(1) / 2, 1),
    );
}
pub(super) fn available(view: &Console<'_>) -> bool {
    studio_view::live(view.runtime, crate::analysis::now_ms())
        && view.runtime["profile_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty())
}
fn editable(view: &Console<'_>) -> bool {
    available(view)
        && view.configuration.is_none_or(|e| {
            !e.invalidated()
                && e.draft.as_ref().is_none_or(|d| {
                    d.domain == control_panel::edit_target(view.sound_row)
                        && d.group == Group::of(view.sound_row)
                })
        })
}
fn hint(selected: usize, view: &Console<'_>) -> &'static str {
    match selected {
        11 => "Bypass affects all outputs; safety gain and limiter remain.",
        14 => "This switch also enables or disables this device's correction.",
        15 => "A/B matching only attenuates; turning it off can change comparison level.",
        9 => "Compression changes dynamics; no automatic makeup gain.",
        _ if view.snapshot.profile.bypass || !view.music.enabled || view.music.reference => {
            "Playback stays as selected; editing tone does not enable it."
        }
        _ => "Edit here; the list only selects. Apply is separate below.",
    }
}
pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let a = layout(area);
    let draft = view.configuration.and_then(|editor| editor.draft.as_ref());
    let mut snapshot = view.snapshot.clone();
    if let Some(d) = draft {
        snapshot.profile = d.eq.clone();
    }
    let staged = Console {
        snapshot: &snapshot,
        music: draft.map_or(view.music, |d| &d.music),
        ..*view
    };
    let mut baseline = view.snapshot.clone();
    if let Some(d) = draft {
        baseline.profile = d.eq_before.clone();
    }
    let before_view = Console {
        snapshot: &baseline,
        music: draft.map_or(view.music, |d| &d.music_before),
        ..*view
    };
    let current_rows = dashboard::sound_rows(&before_view);
    let draft_rows = dashboard::sound_rows(&staged);
    let selected = view.sound_row.min(27);
    let group = Group::of(selected);
    let pending = draft.is_some_and(Draft::dirty);
    let invalid = draft.is_some_and(|d| d.invalidated);
    let enabled = editable(view);
    frame.render_widget(Block::default().style(Style::default().bg(p.panel)), area);
    studio_view::heading(frame, row(a.browser, 0, 1), "Sound settings", p);
    for (index, (rect, section)) in group_rects(a.browser)
        .into_iter()
        .zip(Group::ALL)
        .enumerate()
    {
        let chosen = group == section;
        let unlocked = draft.is_none_or(|d| d.group == section);
        put(
            frame,
            rect,
            &format!(
                "{} {}  {}",
                index + 1,
                if chosen { "▸" } else { " " },
                t(section.label())
            ),
            Style::default()
                .fg(if chosen {
                    p.accent
                } else if unlocked {
                    p.text
                } else {
                    p.muted
                })
                .bg(if chosen { p.selected } else { p.panel })
                .add_modifier(Modifier::BOLD),
        );
    }
    put(
        frame,
        row(a.browser, 6, 1),
        t("Select a parameter"),
        Style::default().fg(p.muted),
    );
    let visible = browser_rows(a.browser, selected);
    for (id, rect) in &visible {
        let active = *id == selected;
        let changed = current_rows[*id].1 != draft_rows[*id].1;
        let columns = Layout::horizontal([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(if *id >= 16 { 14 } else { 8 }),
        ])
        .split(*rect);
        if active {
            frame.render_widget(
                Block::default().style(Style::default().bg(p.selected)),
                *rect,
            );
        }
        put(
            frame,
            columns[0],
            if active {
                "▶"
            } else if changed {
                "•"
            } else {
                " "
            },
            Style::default().fg(p.accent),
        );
        put(
            frame,
            columns[1],
            if *id == 11 {
                t("Global bypass")
            } else {
                &draft_rows[*id].0
            },
            Style::default().fg(if active { p.accent } else { p.text }),
        );
        frame.render_widget(
            Paragraph::new(fit(&draft_rows[*id].1, columns[2].width))
                .alignment(Alignment::Right)
                .style(Style::default().fg(if changed { p.warning } else { p.muted })),
            columns[2],
        );
    }
    let footer = if visible.len() < group.rows().len() {
        format!(
            "{}  {}/{}",
            t("Arrows / wheel: select only"),
            group
                .rows()
                .iter()
                .position(|id| *id == selected)
                .unwrap_or(0)
                + 1,
            group.rows().len()
        )
    } else {
        t("Arrows / wheel: select only").into()
    };
    put(
        frame,
        row(a.browser, a.browser.height.saturating_sub(1), 1),
        &footer,
        Style::default().fg(p.muted),
    );
    let target_scope = draft.map_or(control_panel::edit_target(selected), |d| d.domain);
    let target_output = draft.map_or(view.runtime, |d| &d.runtime);
    let title = if target_scope == UndoTarget::Listening {
        format!(
            "{} · {}",
            t("This output"),
            target_output["output"].as_str().unwrap_or(t("Unknown"))
        )
    } else {
        t(if group == Group::Playback {
            "Playback · all outputs"
        } else {
            "Global EQ · all outputs"
        })
        .into()
    };
    put(
        frame,
        row(a.target, 0, 1),
        &title,
        Style::default()
            .fg(p.secondary)
            .add_modifier(Modifier::BOLD),
    );
    put(
        frame,
        row(a.target, 1, 1),
        t(if invalid {
            "Configuration changed; cancel this draft"
        } else if !available(view) {
            "No current audio telemetry"
        } else if pending {
            "Draft only · sound unchanged"
        } else {
            "Browse safely · nothing changes until Apply"
        }),
        Style::default().fg(if invalid || pending {
            p.warning
        } else {
            p.muted
        }),
    );
    put(
        frame,
        row(a.target, 2, 1),
        t("Routing and correction stay separate"),
        Style::default().fg(p.muted),
    );
    if a.analyzer.height >= 4 {
        if group == Group::Playback {
            frame.render_widget(Paragraph::new(t("Playback switches are separate from tone. Choose OFF or ON, review the scope, then apply."))
                .wrap(Wrap { trim: true }).style(Style::default().fg(p.muted)),
                row(a.analyzer, 1, a.analyzer.height.saturating_sub(1)));
        } else {
            studio_view::draw_analyzer(
                frame,
                a.analyzer,
                &staged,
                studio_view::live(view.runtime, crate::analysis::now_ms()),
                crate::analysis::now_ms(),
            );
            // Clear the original analyzer heading, including wide-character continuation cells.
            let legend = row(a.analyzer, 0, 1);
            frame.render_widget(Clear, legend);
            frame.render_widget(Block::default().style(Style::default().bg(p.panel)), legend);
            put(
                frame,
                legend,
                t(if draft.is_some_and(|d| d.eq != d.eq_before) {
                    "Draft EQ · calculated, not applied"
                } else {
                    "Profile EQ · calculated"
                }),
                Style::default().fg(p.secondary),
            );
        }
    }
    put(
        frame,
        row(a.station, 0, 1),
        if selected == 11 {
            t("Global bypass")
        } else {
            &current_rows[selected].0
        },
        Style::default().fg(p.text).add_modifier(Modifier::BOLD),
    );
    frame.render_widget(
        Paragraph::new(t(hint(selected, &staged)))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(if is_switch(selected) {
                p.warning
            } else {
                p.muted
            })),
        row(a.station, 1, 2),
    );
    let value_area = Rect::new(
        a.minus.right() + 2,
        a.minus.y,
        a.plus.x.saturating_sub(a.minus.right() + 4),
        a.minus.height,
    );
    let values = Layout::horizontal([Constraint::Percentage(50); 2])
        .spacing(1)
        .split(value_area);
    for (rect, label, value) in [
        (values[0], "Current", &current_rows[selected].1),
        (values[1], "Draft", &draft_rows[selected].1),
    ] {
        put(
            frame,
            row(rect, 0, 1),
            t(label),
            Style::default().fg(p.muted),
        );
        put(
            frame,
            row(rect, 1, 1),
            value,
            Style::default().fg(p.text).add_modifier(Modifier::BOLD),
        );
    }
    button(
        frame,
        a.minus,
        if is_switch(selected) { t("OFF") } else { "−" },
        view,
        enabled,
        false,
    );
    button(
        frame,
        a.plus,
        if is_switch(selected) { t("ON") } else { "+" },
        view,
        enabled,
        false,
    );
    if (EQ_ROW_START..EQ_ROW_START + 10).contains(&selected) {
        put(
            frame,
            row(a.station, 6, 1),
            t("Q: bandwidth"),
            Style::default().fg(p.muted),
        );
        button(frame, a.q_minus, "[ Q−", view, enabled, false);
        button(frame, a.q_plus, "] Q+", view, enabled, false);
    }
    let mut changes = Vec::new();
    if let Some(d) = draft {
        if d.domain == UndoTarget::Listening {
            changes = crate::tuning::planner::describe_changes(&d.music_before, &d.music)
                .iter()
                .map(|line| crate::i18n::change(line))
                .collect();
        } else {
            for id in [16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 10, 12, 13, 11] {
                if current_rows[id].1 != draft_rows[id].1 {
                    changes.push(format!(
                        "{}: {} → {}",
                        if id == 11 {
                            t("Global bypass")
                        } else {
                            &draft_rows[id].0
                        },
                        current_rows[id].1,
                        draft_rows[id].1
                    ));
                }
            }
        }
    }
    let count = usize::from(a.review.height.saturating_sub(1));
    let offset = view
        .configuration
        .map_or(0, |editor| editor.review_scroll)
        .min(changes.len().saturating_sub(count));
    let title = if changes.len() > count {
        format!("{} · {} · PgUp/PgDn", t("Pending changes"), changes.len())
    } else {
        format!("{} · {}", t("Pending changes"), changes.len())
    };
    put(
        frame,
        row(a.review, 0, 1),
        &title,
        Style::default().fg(p.secondary),
    );
    for (index, line) in changes.iter().skip(offset).take(count).enumerate() {
        put(
            frame,
            row(a.review, index as u16 + 1, 1),
            line,
            Style::default().fg(p.text),
        );
    }
    if changes.is_empty() {
        put(
            frame,
            row(a.review, 1, 1),
            t("Use + / − to stage a change"),
            Style::default().fg(p.muted),
        );
    }
    button(
        frame,
        a.apply,
        t("Enter Apply changes"),
        view,
        pending && !invalid && available(view),
        true,
    );
    button(
        frame,
        a.cancel,
        t(if pending {
            "Esc Cancel draft"
        } else {
            "Esc Studio"
        }),
        view,
        true,
        false,
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(p.edge)),
        Rect::new(a.browser.right(), area.y, 1, area.height),
    );
}

/// Stateless navigation/draft hit testing. Apply is exclusively handled by PointerLatch.
pub fn pointer_action(area: Rect, view: &Console<'_>, event: MouseEvent) -> Option<PointerAction> {
    if view.workspace != Workspace::Sound
        || view.overlay != Overlay::None
        || control_panel::compact_layout(area.width, area.height)
        || !event.modifiers.is_empty()
    {
        return None;
    }
    let a = layout(studio_controls::shell(area).content);
    if event.kind == MouseEventKind::Down(MouseButton::Left) {
        for (rect, section) in group_rects(a.browser).into_iter().zip(Group::ALL) {
            if rect.contains((event.column, event.row).into()) {
                return Some(PointerAction::Key(section.key()));
            }
        }
        for (id, rect) in browser_rows(a.browser, view.sound_row) {
            if rect.contains((event.column, event.row).into()) {
                return Some(PointerAction::SelectSound(id));
            }
        }
        let enabled = editable(view);
        for (rect, key, allowed) in [
            (a.minus, KeyCode::Char('-'), enabled),
            (a.plus, KeyCode::Char('+'), enabled),
            (
                a.q_minus,
                KeyCode::Char('['),
                enabled && (16..26).contains(&view.sound_row),
            ),
            (
                a.q_plus,
                KeyCode::Char(']'),
                enabled && view.sound_row >= 16,
            ),
            (a.cancel, KeyCode::Esc, true),
        ] {
            if allowed && rect.contains((event.column, event.row).into()) {
                return Some(PointerAction::Key(key));
            }
        }
    }
    if a.browser.contains((event.column, event.row).into()) {
        return match event.kind {
            MouseEventKind::ScrollUp => Some(PointerAction::Key(KeyCode::Up)),
            MouseEventKind::ScrollDown => Some(PointerAction::Key(KeyCode::Down)),
            _ => None,
        };
    }
    if a.review.contains((event.column, event.row).into()) {
        return match event.kind {
            MouseEventKind::ScrollUp => Some(PointerAction::Key(KeyCode::PageUp)),
            MouseEventKind::ScrollDown => Some(PointerAction::Key(KeyCode::PageDown)),
            _ => None,
        };
    }
    None
}
