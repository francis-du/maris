//! Pointer confirmation belongs to the same visible draft on both press and release.
//! A stray down, drag, resize, key, changed selection or changed draft cannot commit it.
use super::view;
use crate::{
    ui::tui::input::{self as control_panel, Overlay, Workspace},
    ui::tui::studio::controls::{self as studio_controls, PointerAction},
    ui::tui::view::Console,
};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[derive(Default)]
pub struct PointerLatch {
    armed: Option<(Rect, usize, u64)>,
}
impl PointerLatch {
    pub fn reset(&mut self) {
        self.armed = None;
    }
    pub fn handle(
        &mut self,
        area: Rect,
        state: &Console<'_>,
        event: MouseEvent,
    ) -> Option<PointerAction> {
        if state.workspace != Workspace::Sound
            || state.overlay != Overlay::None
            || control_panel::compact_layout(area.width, area.height)
            || !event.modifiers.is_empty()
        {
            self.reset();
            return None;
        }
        let target = view::layout(studio_controls::shell(area).content).apply;
        let editor = state.configuration;
        let ready =
            view::available(state) && editor.is_some_and(|e| e.pending() && !e.invalidated());
        let token = editor.map_or(0, |e| e.generation);
        let inside = target.contains((event.column, event.row).into());
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.reset();
                if ready && inside {
                    self.armed = Some((target, state.sound_row, token));
                    return None;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let armed = self.armed.take();
                return (ready && inside && armed == Some((target, state.sound_row, token)))
                    .then_some(PointerAction::Key(KeyCode::Enter));
            }
            MouseEventKind::Moved => {}
            _ => self.reset(),
        }
        view::pointer_action(area, state, event)
    }
}
