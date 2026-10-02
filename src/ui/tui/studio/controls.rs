//! Shared Studio geometry and pointer policy. No state writes or audio operations live here.
//! Mouse adjustments become the same key actions as keyboard adjustments; pickers still require Enter.

use crate::ui::tui::input::{self as control_panel, Overlay, Workspace, DASHBOARD_SOUND_ROWS};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct Shell {
    pub header: Rect,
    pub actions: Rect,
    pub content: Rect,
    pub footer: Rect,
}

pub fn shell(area: Rect) -> Shell {
    let zones = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .margin(1)
    .split(area);
    Shell {
        header: zones[0],
        actions: zones[1],
        content: zones[2],
        footer: zones[3],
    }
}

pub fn actions(area: Rect) -> [Rect; 6] {
    let row = Rect::new(area.x, area.y, area.width, u16::from(area.height > 0));
    let parts = Layout::horizontal([Constraint::Ratio(1, 6); 6])
        .spacing(1)
        .split(row);
    std::array::from_fn(|index| parts[index])
}

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub brand: Rect,
    pub output: Rect,
    pub compare: Rect,
    pub preset: Rect,
    pub status: Rect,
}

pub fn header(area: Rect) -> Header {
    let top = Rect::new(area.x, area.y, area.width, u16::from(area.height > 0));
    let first = Layout::horizontal([
        Constraint::Length(17),
        Constraint::Min(1),
        Constraint::Length(34),
    ])
    .spacing(2)
    .split(top);
    let second = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        u16::from(area.height > 1),
    );
    let bottom = Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
        .spacing(2)
        .split(second);
    Header {
        brand: first[0],
        output: first[1],
        compare: first[2],
        preset: bottom[0],
        status: bottom[1],
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rail {
    pub device: Rect,
    pub controls: Rect,
    pub first: usize,
    pub count: usize,
    pub stride: u16,
}

impl Rail {
    pub fn parameter(self, index: usize) -> Option<Rect> {
        if index < self.first || index >= self.first + self.count {
            return None;
        }
        Some(Rect::new(
            self.controls.x,
            self.controls.y + 2 + (index - self.first) as u16 * self.stride,
            self.controls.width,
            self.stride,
        ))
    }
}

pub fn rail(area: Rect, selected: usize) -> Rail {
    let pad = 1.min(area.width / 2);
    let inner = Rect::new(
        area.x + pad,
        area.y,
        area.width.saturating_sub(pad * 2),
        area.height,
    );
    let device_height = if inner.height >= 34 {
        7
    } else if inner.height >= 20 {
        5
    } else {
        4
    };
    let regions = Layout::vertical([Constraint::Length(device_height), Constraint::Min(1)])
        .spacing(1)
        .split(inner);
    let available = regions[1].height.saturating_sub(2);
    let stride = if available >= 20 { 2 } else { 1 };
    let count = usize::from(available / stride).min(DASHBOARD_SOUND_ROWS);
    let selected = selected.min(DASHBOARD_SOUND_ROWS - 1);
    Rail {
        device: regions[0],
        controls: regions[1],
        first: (selected + 1).saturating_sub(count),
        count,
        stride,
    }
}

/// Named fields and +/- targets share exactly the rectangles that are rendered.
#[derive(Clone, Copy, Debug)]
pub struct Parameter {
    pub pointer: Rect,
    pub label: Rect,
    pub decrement: Rect,
    pub value: Rect,
    pub increment: Rect,
}

pub fn parameter(area: Rect) -> Parameter {
    let top = Rect::new(area.x, area.y, area.width, u16::from(area.height > 0));
    let parts = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(8),
        Constraint::Length(2),
    ])
    .split(top);
    Parameter {
        pointer: parts[0],
        label: parts[1],
        decrement: parts[2],
        value: parts[3],
        increment: parts[4],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    SelectSound(usize),
    SelectHomeSound(usize),
    AdjustListening { row: usize, direction: i8 },
    SoundKey { row: usize, key: KeyCode },
    SelectApp(usize),
    ToggleApp(usize),
    Navigate(Workspace),
    PreviewGoal(usize),
    Key(KeyCode),
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

pub fn pointer_action(
    area: Rect,
    workspace: Workspace,
    overlay: Overlay,
    selected: usize,
    event: MouseEvent,
) -> Option<PointerAction> {
    if overlay != Overlay::None
        || control_panel::compact_layout(area.width, area.height)
        || !event.modifiers.is_empty()
    {
        return None;
    }
    let shell = shell(area);
    let header = header(shell.header);
    let studio = crate::ui::tui::studio::layout(shell.content);
    let rail = rail(studio.rail, selected);
    if event.kind == MouseEventKind::Down(MouseButton::Left) {
        for (rect, destination) in actions(shell.actions).into_iter().zip(Workspace::ALL) {
            if contains(rect, event.column, event.row) {
                return Some(PointerAction::Navigate(destination));
            }
        }
        for (rect, key) in [
            (header.output, KeyCode::Char('o')),
            (header.preset, KeyCode::Char('p')),
            (header.compare, KeyCode::Char('b')),
        ] {
            if contains(rect, event.column, event.row) {
                if workspace == Workspace::Sound && key == KeyCode::Char('b') {
                    return None;
                }
                return Some(PointerAction::Key(key));
            }
        }
    }
    if workspace != Workspace::Now {
        return None;
    }
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            for (rect, destination) in [
                (rail.device, Workspace::Device),
                (studio.analyzer, Workspace::Sound),
                (studio.context, Workspace::Intelligence),
                (studio.intelligence, Workspace::Intelligence),
                (studio.mixer, Workspace::Apps),
                (studio.health, Workspace::System),
            ] {
                // Only the visible heading is a navigation target; plots are not gain controls.
                let offset = u16::from(matches!(destination, Workspace::Apps | Workspace::System));
                let heading = Rect::new(rect.x, rect.y + offset, rect.width, 1);
                if contains(heading, event.column, event.row) {
                    return Some(PointerAction::Navigate(destination));
                }
            }
            for index in rail.first..rail.first + rail.count {
                let rect = rail.parameter(index)?;
                if !contains(rect, event.column, event.row) {
                    continue;
                }
                let parts = parameter(rect);
                // +/- are visible only on the already selected row. A click on any other row
                // selects it without altering sound; range-track clicks never jump the gain.
                if index == selected {
                    if contains(parts.decrement, event.column, event.row) {
                        return Some(PointerAction::AdjustListening {
                            row: index,
                            direction: -1,
                        });
                    }
                    if contains(parts.increment, event.column, event.row) {
                        return Some(PointerAction::AdjustListening {
                            row: index,
                            direction: 1,
                        });
                    }
                }
                return Some(PointerAction::SelectSound(index));
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(rail.controls, event.column, event.row) =>
        {
            let key = if event.kind == MouseEventKind::ScrollUp {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            return Some(PointerAction::Key(key));
        }
        _ => {}
    }
    None
}

/// Shared by the terminal and menu bar. None means the mixer acknowledged this revision.
pub(crate) fn mixer_application_state(
    runtime: &Value,
    requested: Option<u64>,
) -> Option<&'static str> {
    if runtime["system_backend"] != "multi_device_mixer" {
        return None;
    }
    match runtime["mixer"]["restart_required"].as_bool() {
        Some(true) => {
            return Some("Mixer assignments changed; stop and restart before adjusting channels")
        }
        Some(false) => {}
        None => return Some("Apply state unknown"),
    }
    match (requested, runtime["mixer"]["processing_revision"].as_u64()) {
        (Some(saved), Some(applied)) if saved == applied => None,
        (Some(_), Some(_)) => Some("Pending audio update"),
        _ => Some("Apply state unknown"),
    }
}

/// Read-only status: a saved profile is not proof that the audio callback has applied it.
pub fn application_state(runtime: &Value, profile_revision: u64, now: u64) -> &'static str {
    if runtime["active"] != true {
        return "Saved offline";
    }
    if !crate::ui::tui::studio::live(runtime, now) {
        return "Awaiting telemetry";
    }
    let Some(requested) = runtime["requested_music_revision"].as_u64() else {
        return "Apply state unknown";
    };
    if runtime["settings_pending"] == true {
        return "Pending audio update";
    }
    let mixer_revision = runtime["mixer_control"]["revision"]
        .as_u64()
        .or_else(|| runtime["mixer"]["revision"].as_u64());
    if let Some(status) = mixer_application_state(runtime, mixer_revision) {
        return status;
    }
    match (
        runtime["applied_revision"].as_u64(),
        runtime["applied_music_revision"].as_u64(),
    ) {
        (Some(profile), Some(music)) if profile == profile_revision && music == requested => {
            "Applied"
        }
        (Some(_), Some(_)) => "Pending audio update",
        _ => "Apply state unknown",
    }
}
