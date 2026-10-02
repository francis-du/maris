use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use maris::{
    control_panel::{Overlay, Workspace},
    studio_controls::{self as controls, PointerAction},
};
use ratatui::layout::Rect;
use serde_json::json;

fn event(kind: MouseEventKind, rect: Rect) -> MouseEvent {
    MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}
fn click(area: Rect, selected: usize, target: Rect) -> Option<PointerAction> {
    controls::pointer_action(
        area,
        Workspace::Now,
        Overlay::None,
        selected,
        event(MouseEventKind::Down(MouseButton::Left), target),
    )
}

#[test]
fn every_rendered_parameter_is_clickable_across_size_and_scroll_variants() {
    for (w, h) in [(90, 26), (100, 32), (120, 36), (140, 40), (180, 50)] {
        let area = Rect::new(0, 0, w, h);
        let studio = maris::studio_view::layout(controls::shell(area).content);
        for selected in 0..10 {
            let rail = controls::rail(studio.rail, selected);
            for index in rail.first..rail.first + rail.count {
                let row = rail.parameter(index).unwrap();
                let parts = controls::parameter(row);
                assert_eq!(
                    click(area, selected, parts.label),
                    Some(PointerAction::SelectSound(index))
                );
                assert_eq!(
                    click(area, selected, parts.increment),
                    Some(if index == selected {
                        PointerAction::AdjustListening {
                            row: index,
                            direction: 1,
                        }
                    } else {
                        PointerAction::SelectSound(index)
                    })
                );
                if rail.stride == 2 {
                    assert_eq!(
                        click(area, selected, Rect::new(row.x + 2, row.y + 1, 1, 1)),
                        Some(PointerAction::SelectSound(index))
                    );
                }
            }
        }
    }
}

#[test]
fn pointer_targets_match_rendered_plus_minus_cells_and_reuse_validated_revision_undo() {
    let area = Rect::new(0, 0, 140, 40);
    let studio = maris::studio_view::layout(controls::shell(area).content);
    let rail = controls::rail(studio.rail, 0);
    let parts = controls::parameter(rail.parameter(0).unwrap());
    let directory = tempfile::tempdir().unwrap();
    let store = maris::store::Store::at(directory.path());
    let initial = maris::listening::load(&store).unwrap();
    let action = click(area, 0, parts.increment).unwrap();
    let direction = match action {
        PointerAction::AdjustListening { row: 0, direction } => f64::from(direction),
        _ => panic!("unexpected pointer action"),
    };
    maris::music_view::adjust(&store, Some("Headphones"), initial.revision, 0, direction).unwrap();
    let changed = maris::listening::load(&store).unwrap();
    assert_eq!(changed.effective("Headphones").bass_db, 0.5);
    assert!(
        maris::music_view::adjust(&store, Some("Headphones"), initial.revision, 0, direction)
            .is_err()
    );
    let undone = maris::listening::undo(&store, Some(changed.revision)).unwrap();
    assert_eq!(undone.effective("Headphones").bass_db, 0.0);
    assert_eq!(
        click(area, 0, parts.decrement),
        Some(PointerAction::AdjustListening {
            row: 0,
            direction: -1,
        })
    );
}

#[test]
fn header_pointer_actions_only_open_pickers_or_request_existing_ab_action() {
    let area = Rect::new(0, 0, 140, 40);
    let header = controls::header(controls::shell(area).header);
    assert_eq!(
        click(area, 0, header.output),
        Some(PointerAction::Key(KeyCode::Char('o')))
    );
    assert_eq!(
        click(area, 0, header.preset),
        Some(PointerAction::Key(KeyCode::Char('p')))
    );
    assert_eq!(
        click(area, 0, header.compare),
        Some(PointerAction::Key(KeyCode::Char('b')))
    );
    assert_eq!(click(area, 0, header.status), None);
}

#[test]
fn modal_compact_and_detail_views_block_underlying_pointer_actions() {
    let area = Rect::new(0, 0, 140, 40);
    let studio = maris::studio_view::layout(controls::shell(area).content);
    let plus = controls::parameter(controls::rail(studio.rail, 0).parameter(0).unwrap()).increment;
    for overlay in [
        Overlay::Help,
        Overlay::Output,
        Overlay::Preset,
        Overlay::Proposal,
        Overlay::StartSystem,
    ] {
        assert_eq!(
            controls::pointer_action(
                area,
                Workspace::Now,
                overlay,
                0,
                event(MouseEventKind::Down(MouseButton::Left), plus)
            ),
            None
        );
    }
    for workspace in Workspace::DETAILS {
        assert_eq!(
            controls::pointer_action(
                area,
                workspace,
                Overlay::None,
                0,
                event(MouseEventKind::Down(MouseButton::Left), plus)
            ),
            None
        );
    }
    for area in [Rect::new(0, 0, 80, 24), Rect::new(0, 0, 0, 0)] {
        assert_eq!(click(area, 0, plus), None);
    }
}

#[test]
fn scrolling_selects_controls_without_changing_values_and_drag_is_inert() {
    let area = Rect::new(0, 0, 100, 32);
    let studio = maris::studio_view::layout(controls::shell(area).content);
    let target = controls::rail(studio.rail, 4).controls;
    assert_eq!(
        controls::pointer_action(
            area,
            Workspace::Now,
            Overlay::None,
            4,
            event(MouseEventKind::ScrollDown, target)
        ),
        Some(PointerAction::Key(KeyCode::Down))
    );
    assert_eq!(
        controls::pointer_action(
            area,
            Workspace::Now,
            Overlay::None,
            4,
            event(MouseEventKind::ScrollUp, studio.analyzer)
        ),
        None
    );
    for kind in [
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
    ] {
        assert_eq!(
            controls::pointer_action(area, Workspace::Now, Overlay::None, 4, event(kind, target)),
            None
        );
    }
}

#[test]
fn applied_badge_requires_both_callback_revisions_and_current_telemetry() {
    let mut runtime = json!({"active":true,"updated_at_ms":1000,"requested_music_revision":7,"applied_music_revision":7,"applied_revision":3});
    assert_eq!(controls::application_state(&runtime, 3, 1200), "Applied");
    assert_eq!(
        controls::application_state(&runtime, 4, 1200),
        "Pending audio update"
    );
    runtime["applied_music_revision"] = json!(6);
    assert_eq!(
        controls::application_state(&runtime, 3, 1200),
        "Pending audio update"
    );
    runtime["applied_music_revision"] = serde_json::Value::Null;
    assert_eq!(
        controls::application_state(&runtime, 3, 1200),
        "Apply state unknown"
    );
    runtime["active"] = json!(false);
    assert_eq!(
        controls::application_state(&runtime, 3, 1200),
        "Saved offline"
    );
}
