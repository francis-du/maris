//! Shared output-picker geometry and read-only pointer selection.
//! A pointer click only changes the highlighted row; Enter still confirms routing.
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Margin, Rect};

#[derive(Clone, Copy, Debug)]
pub struct Areas {
    pub modal: Rect,
    pub search: Rect,
    pub table: Rect,
    pub rows: Rect,
    pub offset: usize,
}

pub fn layout(area: Rect, count: usize, selected: usize) -> Areas {
    let width = 70.min(area.width.saturating_sub(2));
    let desired = (count.max(1).saturating_add(5)).min(u16::MAX as usize) as u16;
    let height = desired.min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(Margin::new(1, 1));
    let table = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    );
    let visible = usize::from(table.height.saturating_sub(1)).min(count);
    let offset = selected
        .min(count.saturating_sub(1))
        .saturating_add(1)
        .saturating_sub(visible);
    Areas {
        modal,
        search: Rect::new(inner.x, inner.y, inner.width, inner.height.min(1)),
        table,
        rows: Rect::new(
            table.x,
            table.y.saturating_add(1),
            table.width,
            visible as u16,
        ),
        offset,
    }
}

pub fn pointer_choice_filtered(
    area: Rect,
    devices: &[crate::audio::DeviceInfo],
    selected: usize,
    invalidated: bool,
    search: &super::picker_search::Search,
    event: MouseEvent,
) -> Option<usize> {
    let choices = search.outputs(devices);
    let position = choices
        .iter()
        .position(|index| *index == selected)
        .unwrap_or(0);
    pointer_choice(area, choices.len(), position, invalidated, event)
        .and_then(|row| choices.get(row).copied())
}

pub fn pointer_choice(
    area: Rect,
    count: usize,
    selected: usize,
    invalidated: bool,
    event: MouseEvent,
) -> Option<usize> {
    if invalidated
        || count == 0
        || area.width < 30
        || area.height < 8
        || !event.modifiers.is_empty()
    {
        return None;
    }
    let areas = layout(area, count, selected);
    let last = count - 1;
    match event.kind {
        MouseEventKind::Down(MouseButton::Left)
            if areas.rows.contains((event.column, event.row).into()) =>
        {
            Some((areas.offset + usize::from(event.row - areas.rows.y)).min(last))
        }
        MouseEventKind::ScrollUp if areas.modal.contains((event.column, event.row).into()) => {
            Some(selected.min(last).saturating_sub(1))
        }
        MouseEventKind::ScrollDown if areas.modal.contains((event.column, event.row).into()) => {
            Some(selected.saturating_add(1).min(last))
        }
        _ => None,
    }
}
