//! Click delivery regressions use actual input policy/reducers, never live audio.
#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use maris::{
    control_panel::{self, PressGate},
    desktop_controls::{Controller, Summary},
    listening,
    store::Store,
};
use serde_json::json;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

fn mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 110,
        row: 20,
        modifiers: KeyModifiers::NONE,
    })
}
fn fixture() -> (tempfile::TempDir, Store) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    store.write_json("runtime.json", &json!({
        "active":true,"music_processing":true,"session_id":"click-fixture",
        "profile_key":"Headphones","output":"Headphones","sample_rate":48000,
        "updated_at_ms":maris::analysis::now_ms(),"applied_revision":0,"applied_music_revision":0
    })).unwrap();
    (temp, store)
}

#[test]
fn runtime_status_uses_the_same_debug_clock_as_live_fixture_heartbeats() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    store
        .write_json(
            "runtime.json",
            &json!({"active":true,"updated_at_ms":1_u64}),
        )
        .unwrap();
    let _clock = maris::analysis::DebugClock::freeze_at(1);
    let status = maris::audio::runtime_status(&store);
    assert_eq!(status["active"], true);
    assert_ne!(status["stale"], true);
}

fn observe_live(controller: &mut Controller, store: &Store) -> maris::analysis::DebugClock {
    let runtime = live_telemetry::refresh(store);
    let now = runtime["updated_at_ms"].as_u64().unwrap();
    let clock = maris::analysis::DebugClock::freeze_at(now);
    controller.observe(&Summary::read(store, &runtime, now).unwrap());
    clock
}

#[test]
fn motion_events_do_not_require_one_redraw_each_before_a_button_press() {
    let down = mouse(MouseEventKind::Down(MouseButton::Left));
    let up = mouse(MouseEventKind::Up(MouseButton::Left));
    let mut events = VecDeque::from(vec![mouse(MouseEventKind::Moved); 100]);
    events.extend([down.clone(), up.clone()]);
    assert_eq!(
        control_panel::next_input(|| Ok(events.pop_front())).unwrap(),
        Some(down)
    );
    assert_eq!(
        control_panel::next_input(|| Ok(events.pop_front())).unwrap(),
        Some(up)
    );
    assert!(events.is_empty());
    assert!(include_str!("../src/ui/tui/mod.rs").contains("control_panel::read_input()"));
}

#[test]
fn a_deliberate_mouse_confirmation_is_not_a_keyboard_auto_repeat() {
    let mut gate = PressGate::default();
    let now = Instant::now();
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(gate.accept(enter, now));
    gate.shield_confirmation();
    assert!(!gate.accept(enter, now + Duration::from_millis(20)));
    gate.pointer_input(); // A new physical press precedes the paired Apply release.
    assert!(gate.accept(enter, now + Duration::from_millis(100)));
}

#[test]
fn enter_after_a_deliberate_adjustment_is_not_dropped_as_repeat() {
    let now = Instant::now();
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    for shield in [false, true] {
        for key in ['+', '-', '[', ']'] {
            let mut gate = PressGate::default();
            assert!(gate.accept(enter, now));
            if shield {
                gate.shield_confirmation();
            }
            assert!(gate.accept(
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                now + Duration::from_millis(40)
            ));
            assert!(
                gate.accept(enter, now + Duration::from_millis(80)),
                "fresh Enter after {key} was silently discarded (shield={shield})"
            );
            assert!(!gate.accept(enter, now + Duration::from_millis(100)));
        }
    }
}

#[test]
fn successive_native_menu_actions_do_not_conflict_with_their_own_saved_revision() {
    let (_temp, store) = fixture();
    let mut controller = Controller::default();
    let _clock = observe_live(&mut controller, &store);
    live_telemetry::refresh(&store);
    controller.compare(&store).unwrap();
    live_telemetry::refresh(&store);
    controller.compare(&store).unwrap();
    assert_eq!(listening::load(&store).unwrap().revision, 2);
    assert!(
        !listening::load(&store)
            .unwrap()
            .effective("Headphones")
            .reference
    );
    live_telemetry::refresh(&store);
    controller.toggle_processing(&store).unwrap();
    live_telemetry::refresh(&store);
    controller.toggle_processing(&store).unwrap();
    assert_eq!(store.load().unwrap().revision, 2);
    assert!(!store.load().unwrap().profile.bypass);
}

#[test]
fn confirmed_menu_scene_can_be_compared_immediately_without_a_refresh() {
    // Audio heartbeats advance; the controller labels are never re-observed.
    let (_temp, store) = fixture();
    let mut controller = Controller::default();
    let _clock = observe_live(&mut controller, &store);
    live_telemetry::refresh(&store);
    controller.select_preset(&store, "focus").unwrap();
    live_telemetry::refresh(&store);
    controller.apply(&store, &[]).unwrap();
    assert_eq!(listening::load(&store).unwrap().revision, 1);
    live_telemetry::refresh(&store);
    controller.compare(&store).unwrap();
    assert!(
        listening::load(&store)
            .unwrap()
            .effective("Headphones")
            .reference
    );
    live_telemetry::refresh(&store);
    controller.undo(&store).unwrap();
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("Headphones")
            .bass_db,
        0.0
    );
}

#[test]
fn bursts_keep_every_click_and_resize_boundary_in_order() {
    let mut input = VecDeque::from(vec![mouse(MouseEventKind::Moved); 400]);
    let expected = [
        mouse(MouseEventKind::Down(MouseButton::Left)),
        Event::Resize(100, 32),
        mouse(MouseEventKind::Drag(MouseButton::Left)),
        mouse(MouseEventKind::Up(MouseButton::Left)),
        Event::Key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)),
    ];
    input.extend(expected.iter().cloned());
    let mut received = Vec::new();
    while !input.is_empty() {
        if let Some(event) = control_panel::next_input(|| Ok(input.pop_front())).unwrap() {
            received.push(event);
        }
    }
    assert_eq!(received, expected);
}

#[test]
fn rapid_deliberate_plus_minus_clicks_are_not_throttled_as_keyboard_toggles() {
    let now = Instant::now();
    let mut gate = PressGate::default();
    for i in 0..200 {
        gate.pointer_input();
        let key = KeyEvent::new(
            KeyCode::Char(if i % 2 == 0 { '+' } else { '-' }),
            KeyModifiers::NONE,
        );
        assert!(gate.accept(key, now + Duration::from_millis(i * 10)));
    }
}

#[test]
fn removing_label_revision_guards_would_overwrite_an_external_edit() {
    let (_temp, store) = fixture();
    let mut controller = Controller::default();
    let _clock = observe_live(&mut controller, &store);
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.bass_db = 1.0;
        Ok(())
    })
    .unwrap();
    assert!(controller.compare(&store).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 1);
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("Headphones")
            .bass_db,
        1.0
    );
}
