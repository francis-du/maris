//! One layout for rendered preset rows and read-only pointer selection.
//! Selecting a row never applies it; Enter still uses the guarded transaction.
use super::{input::Overlay, view::Console};
use crate::{control::store::Snapshot, dsp::music::MusicProfile, presets::PresetSummary};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use serde_json::Value;

pub struct Areas {
    pub modal: Rect,
    pub table: Rect,
    pub preview: Rect,
    pub rows: Rect,
    pub offset: usize,
}

fn matches_current(
    preset: &PresetSummary,
    snapshot: &Snapshot,
    music: &MusicProfile,
    runtime: &Value,
) -> bool {
    if preset.category == "scene" {
        let capability =
            serde_json::from_value(runtime["device_capability"].clone()).unwrap_or_default();
        return crate::presets::scenes::prepare(music, &capability, &preset.id)
            .is_ok_and(|preview| preview.profile == *music);
    }
    let rate = runtime["sample_rate"]
        .as_u64()
        .filter(|rate| (44_100..=192_000).contains(rate))
        .unwrap_or(48_000) as u32;
    crate::presets::profile(&preset.id, rate).is_ok_and(|profile| profile == snapshot.profile)
}

/// Keep the last applied picker row when that layer still matches. If another
/// surface changed it, fall back to the first preset that exactly matches the
/// current listening or EQ state instead of resetting to row zero.
pub fn preferred_choice(
    presets: &[PresetSummary],
    preferred: usize,
    snapshot: &Snapshot,
    music: &MusicProfile,
    runtime: &Value,
) -> Option<usize> {
    presets
        .get(preferred)
        .filter(|preset| matches_current(preset, snapshot, music, runtime))
        .map(|_| preferred)
        .or_else(|| {
            presets
                .iter()
                .position(|preset| matches_current(preset, snapshot, music, runtime))
        })
}

pub fn layout(area: Rect, count: usize, selected: usize) -> Areas {
    let width = 84.min(area.width.saturating_sub(2));
    let height = area.height.saturating_sub(4).min(32);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(ratatui::layout::Margin::new(1, 1));
    let preview_height = if inner.height >= 18 { 9 } else { 4 };
    let zones = Layout::vertical([Constraint::Min(2), Constraint::Length(preview_height)])
        .spacing(1)
        .split(inner);
    let visible = usize::from(zones[0].height.saturating_sub(1)).min(count);
    let offset = selected
        .min(count.saturating_sub(1))
        .saturating_add(1)
        .saturating_sub(visible);
    Areas {
        modal,
        table: zones[0],
        preview: zones[1],
        rows: Rect::new(
            zones[0].x,
            zones[0].y.saturating_add(1),
            zones[0].width,
            visible as u16,
        ),
        offset,
    }
}

pub fn pointer_choice(area: Rect, view: &Console<'_>, event: MouseEvent) -> Option<usize> {
    if view.overlay != Overlay::Preset
        || view.runtime["selection_invalidated"] == true
        || view.presets.is_empty()
        || area.width < 30
        || area.height < 8
        || !event.modifiers.is_empty()
    {
        return None;
    }
    let areas = layout(area, view.presets.len(), view.preset_choice);
    if !areas.rows.contains((event.column, event.row).into()) {
        return None;
    }
    let last = view.presets.len() - 1;
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            Some((areas.offset + usize::from(event.row - areas.rows.y)).min(last))
        }
        MouseEventKind::ScrollUp => Some(view.preset_choice.min(last).saturating_sub(1)),
        MouseEventKind::ScrollDown => Some(view.preset_choice.saturating_add(1).min(last)),
        _ => None,
    }
}
