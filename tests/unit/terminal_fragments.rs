//! Test terminal report fragments before they can become UI shortcuts.
use super::{sgr_mouse, TerminalInput};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use std::{collections::VecDeque, time::Duration};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn fragments(report: &str) -> VecDeque<Event> {
    std::iter::once(key(KeyCode::Esc))
        .chain(report.chars().map(|ch| key(KeyCode::Char(ch))))
        .collect()
}
#[test]
fn split_mouse_reports_do_not_leak_escape_brackets_or_coordinates_as_shortcuts() {
    let mut reader = TerminalInput::default();
    let mut input = fragments("[<0;135;22M");
    input.extend(fragments("[<0;135;22m"));
    input.push_back(key(KeyCode::Enter));
    for expected in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let Event::Mouse(mouse) = reader
            .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
            .unwrap()
            .unwrap()
        else {
            panic!("A mouse fragment reached the keyboard handler");
        };
        assert_eq!(mouse.kind, expected);
        assert_eq!((mouse.column, mouse.row), (134, 21));
    }
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
            .unwrap(),
        Some(key(KeyCode::Enter))
    );
    assert!(input.is_empty());
}
#[test]
fn paused_reports_preserve_each_press_and_release_without_coordinate_shortcuts() {
    for report in ["[<0;135;22M", "[<0;135;22m", "[<0;9;22M", "[<0;9;22m"] {
        for split in 2..report.len() {
            let mut reader = TerminalInput::default();
            let mut first = fragments(&report[..split]);
            assert!(reader
                .next(Duration::ZERO, &mut |_| Ok(first.pop_front()))
                .unwrap()
                .is_none());
            for _ in 0..3 {
                assert!(reader
                    .next(Duration::ZERO, &mut |_| Ok(None))
                    .unwrap()
                    .is_none());
            }
            let mut rest: VecDeque<_> = report[split..]
                .chars()
                .map(|ch| key(KeyCode::Char(ch)))
                .collect();
            rest.push_back(key(KeyCode::Enter));
            assert_eq!(
                reader
                    .next(Duration::ZERO, &mut |_| Ok(rest.pop_front()))
                    .unwrap(),
                Some(Event::Mouse(sgr_mouse(report).unwrap())),
                "{report} split {split}"
            );
            assert_eq!(
                reader
                    .next(Duration::ZERO, &mut |_| Ok(rest.pop_front()))
                    .unwrap(),
                Some(key(KeyCode::Enter))
            );
            assert!(reader.queued.is_empty());
            assert!(reader.pending_mouse.is_none());
        }
    }
}

#[test]
fn actual_escape_and_non_mouse_keys_keep_their_original_order() {
    for tail in ["", "q", "[x", "[", "+"] {
        let mut input = fragments(tail);
        let expected: Vec<_> = input.iter().cloned().collect();
        let mut reader = TerminalInput::default();
        let mut actual = Vec::new();
        for _ in 0..expected.len() {
            actual.push(
                reader
                    .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
                    .unwrap()
                    .unwrap(),
            );
        }
        assert_eq!(actual, expected, "{tail}");
    }
}
#[test]
fn completed_native_events_and_resize_are_not_reinterpreted() {
    let mut input = VecDeque::from([
        Event::Mouse(sgr_mouse("[<35;110;20M").unwrap()),
        Event::Resize(140, 40),
        key(KeyCode::Char('+')),
    ]);
    let expected = input.clone();
    let mut reader = TerminalInput::default();
    for event in expected {
        assert_eq!(
            reader
                .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
                .unwrap(),
            Some(event)
        );
    }
}
#[test]
fn malformed_complete_mouse_reports_cannot_produce_keyboard_actions() {
    for report in [
        "[<0;0;1M",
        "[<0;1;0M",
        "[<999;1;1M",
        "[<0;65536;1M",
        "[<0;1;1;2M",
        "[<0;;1M",
    ] {
        let mut reader = TerminalInput::default();
        let mut input = fragments(report);
        assert!(
            reader
                .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
                .unwrap()
                .is_none(),
            "{report}"
        );
        assert!(input.is_empty());
        assert!(reader.queued.is_empty());
    }
}
#[test]
fn incomplete_and_oversized_mouse_tails_never_become_number_shortcuts() {
    let mut reader = TerminalInput::default();
    let mut first = fragments("[<0;12;");
    assert!(reader
        .next(Duration::ZERO, &mut |_| Ok(first.pop_front()))
        .unwrap()
        .is_none());
    let mut rest = VecDeque::from([
        key(KeyCode::Char('2')),
        key(KeyCode::Char('m')),
        key(KeyCode::Char('q')),
    ]);
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(rest.pop_front()))
            .unwrap(),
        Some(Event::Mouse(sgr_mouse("[<0;12;2m").unwrap()))
    );
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(rest.pop_front()))
            .unwrap(),
        Some(key(KeyCode::Char('q')))
    );

    let mut input = fragments(&format!("[<{};1;1M", "1".repeat(160)));
    let mut reader = TerminalInput::default();
    let mut calls = 0;
    while !input.is_empty() {
        assert!(reader
            .next(Duration::ZERO, &mut |_| {
                calls += 1;
                Ok(input.pop_front())
            })
            .unwrap()
            .is_none());
        assert!(calls <= 169);
    }
    assert!(!reader.discard_mouse_tail);
    assert!(reader.queued.is_empty());
}
#[test]
fn resize_interrupting_a_fragment_is_preserved_without_a_fake_escape() {
    let mut reader = TerminalInput::default();
    let mut input = fragments("[<0;1;");
    input.push_back(Event::Resize(90, 30));
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
            .unwrap(),
        Some(Event::Resize(90, 30))
    );
    assert!(reader.queued.is_empty());
}
#[test]
fn resize_mid_report_does_not_release_its_remaining_coordinates_as_keys() {
    let mut reader = TerminalInput::default();
    let mut input = fragments("[<0;1;");
    input.push_back(Event::Resize(90, 30));
    input.extend([
        key(KeyCode::Char('2')),
        key(KeyCode::Char('m')),
        key(KeyCode::Char('q')),
    ]);
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
            .unwrap(),
        Some(Event::Resize(90, 30))
    );
    assert!(reader
        .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
        .unwrap()
        .is_none());
    assert_eq!(
        reader
            .next(Duration::ZERO, &mut |_| Ok(input.pop_front()))
            .unwrap(),
        Some(key(KeyCode::Char('q')))
    );
}

#[test]
fn mouse_button_modifiers_drag_wheel_and_coordinate_bounds_are_preserved() {
    assert_eq!(
        sgr_mouse("[<28;65535;65535M").unwrap().modifiers,
        KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL
    );
    assert_eq!(
        sgr_mouse("[<32;1;1M").unwrap().kind,
        MouseEventKind::Drag(MouseButton::Left)
    );
    assert_eq!(sgr_mouse("[<35;1;1M").unwrap().kind, MouseEventKind::Moved);
    assert_eq!(
        sgr_mouse("[<64;1;1M").unwrap().kind,
        MouseEventKind::ScrollUp
    );
    assert_eq!(
        sgr_mouse("[<65;1;1M").unwrap().kind,
        MouseEventKind::ScrollDown
    );
    assert_eq!(
        sgr_mouse("[<2;1;1m").unwrap().kind,
        MouseEventKind::Up(MouseButton::Right)
    );
    assert!(sgr_mouse("[<64;1;1m").is_none());
}
