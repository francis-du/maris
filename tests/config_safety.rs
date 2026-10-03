#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use maris::{
    analysis, control,
    control_panel::{self, PickerGuard, PressGate},
    listening,
    store::Store,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn runtime() -> Value {
    json!({"active":true,"session_id":"fixture","pid":100,"music_processing":true,
        "updated_at_ms":analysis::now_ms(),"profile_key":"Headphones","output":"Headphones",
        "device_identity":{"stable_id":"A"},"device_binding_revision":1,"sample_rate":48000,
        "rebind_count":0,"system_backend":"coreaudio_process_tap","device_capability":{}})
}

#[test]
fn picker_does_not_silently_retarget_after_device_or_configuration_changes() {
    let before = runtime();
    let now = analysis::now_ms();
    let guard = PickerGuard::capture(&before, 2, 3, now);
    guard.validate(&before, 2, 3, now).unwrap();
    for field in [
        "session_id",
        "profile_key",
        "output",
        "device_identity",
        "device_binding_revision",
        "sample_rate",
        "rebind_count",
        "device_capability",
    ] {
        let mut after = before.clone();
        after[field] = json!("changed");
        assert!(guard.validate(&after, 2, 3, now).is_err(), "missed {field}");
    }
    assert!(guard.validate(&before, 3, 3, now).is_err());
    assert!(guard.validate(&before, 2, 4, now).is_err());
    assert!(guard.validate(&before, 2, 3, now + 60001).is_err());
    assert!(guard.validate(&before, 2, 3, now - 1).is_err());
}

#[test]
fn delayed_pointer_or_key_cannot_edit_a_previous_output_or_an_offline_default() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let displayed = runtime();
    store.write_json("runtime.json", &displayed).unwrap();
    let displayed = live_telemetry::refresh(&store);
    control_panel::ensure_displayed_output(&store, &displayed).unwrap();
    let mut changed = displayed.clone();
    changed["profile_key"] = json!("Speakers");
    store.write_json("runtime.json", &changed).unwrap();
    assert!(control_panel::ensure_displayed_output(&store, &displayed).is_err());
    assert!(control_panel::ensure_displayed_output(&store, &json!({"active":false})).is_err());
    assert!(!dir.path().join("listening.json").exists());
}

#[test]
fn held_discrete_keys_do_not_toggle_confirm_or_undo_repeatedly() {
    let start = Instant::now();
    for code in [
        KeyCode::Enter,
        KeyCode::Char('b'),
        KeyCode::Char('u'),
        KeyCode::Char(' '),
        KeyCode::Char('s'),
    ] {
        let mut gate = PressGate::default();
        assert!(gate.accept(KeyEvent::new(code, KeyModifiers::NONE), start));
        for frame in 1..50 {
            // Legacy terminals often encode auto-repeat as Press rather than Repeat.
            assert!(!gate.accept(
                KeyEvent::new(code, KeyModifiers::NONE),
                start + Duration::from_millis(frame * 30)
            ));
        }
        assert!(gate.accept(
            KeyEvent::new(code, KeyModifiers::NONE),
            start + Duration::from_millis(2000)
        ));
    }
}

#[test]
fn navigation_keeps_repeat_but_modified_sound_shortcuts_are_ignored() {
    let start = Instant::now();
    let mut gate = PressGate::default();
    for frame in 0..10 {
        assert!(gate.accept(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            start + Duration::from_millis(frame)
        ));
    }
    assert!(!gate.accept(
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        start
    ));
    assert!(!gate.accept(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT), start));
    let mut repeat = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    assert!(!gate.accept(repeat, start));
}

#[test]
fn a_confirmation_key_is_not_replayed_into_the_screen_under_a_closed_modal() {
    let start = Instant::now();
    let mut gate = PressGate::default();
    assert!(gate.accept(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), start));
    gate.shield_confirmation();
    for milliseconds in [35, 500, 1000, 10_000] {
        assert!(!gate.accept(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            start + Duration::from_millis(milliseconds)
        ));
    }
    assert!(gate.accept(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        start + Duration::from_millis(10_050)
    ));
    assert!(gate.accept(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        start + Duration::from_millis(10_100)
    ));
}

#[test]
fn explicit_release_or_pointer_input_allows_a_new_confirmation_context() {
    let start = Instant::now();
    let mut gate = PressGate::default();
    gate.shield_confirmation();
    let mut release = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert!(!gate.accept(release, start));
    assert!(gate.accept(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), start));
    gate.shield_confirmation();
    gate.pointer_input();
    assert!(gate.accept(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        start + Duration::from_secs(2)
    ));
}

#[test]
fn no_op_writes_keep_revisions_and_the_last_useful_undo() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.bass_db = 6.0;
        Ok(())
    })
    .unwrap();
    let history = std::fs::read(dir.path().join("listening-previous.json")).unwrap();
    let unchanged = listening::edit(&store, Some(1), Some("Headphones"), |p| {
        p.bass_db = (p.bass_db + 0.5).min(6.0);
        Ok(())
    })
    .unwrap();
    assert_eq!(unchanged.revision, 1);
    assert_eq!(
        std::fs::read(dir.path().join("listening-previous.json")).unwrap(),
        history
    );
    assert!(listening::edit(&store, Some(0), Some("Headphones"), |_| Ok(())).is_err());
    store
        .edit(Some(0), |p| {
            p.preamp_db = -4.0;
            Ok(())
        })
        .unwrap();
    let global = std::fs::read(dir.path().join("profile.json")).unwrap();
    assert_eq!(store.edit(Some(1), |_| Ok(())).unwrap().revision, 1);
    assert_eq!(
        std::fs::read(dir.path().join("profile.json")).unwrap(),
        global
    );
}

#[test]
fn comparison_is_revisioned_without_erasing_listening_undo() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    listening::preset(&store, Some(0), Some("Headphones"), "focus").unwrap();
    let history = std::fs::read(dir.path().join("listening-previous.json")).unwrap();
    assert_eq!(
        listening::compare(&store, Some(1), Some("Headphones"), true)
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        listening::compare(&store, Some(2), Some("Headphones"), true)
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        std::fs::read(dir.path().join("listening-previous.json")).unwrap(),
        history
    );
    listening::undo_device(&store, 2, "Headphones").unwrap();
    assert_eq!(
        listening::load(&store).unwrap().effective("Headphones"),
        &maris::music::MusicProfile::default()
    );
}

#[test]
fn an_unconsumed_route_command_cannot_be_overwritten_but_stop_has_priority() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    store.write_json("runtime.json", &runtime()).unwrap();
    live_telemetry::refresh(&store);
    control::request_output(&store, Some("uid:A")).unwrap();
    let queued = std::fs::read(dir.path().join("control.json")).unwrap();
    live_telemetry::refresh(&store);
    control::request_output(&store, Some("uid:A")).unwrap();
    live_telemetry::refresh(&store);
    assert!(control::request_applications(&store, &[123]).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("control.json")).unwrap(),
        queued
    );
    control::request_stop(&store).unwrap();
    let stop: Value = maris::store::read_json(&dir.path().join("control.json")).unwrap();
    assert_eq!(stop["action"], "stop");
}

#[test]
fn concurrent_interfaces_cannot_silently_replace_each_others_route_requests() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    store.write_json("runtime.json", &runtime()).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["uid:A", "uid:B"]
        .into_iter()
        .map(|id| {
            let barrier = barrier.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (id, control::request_output(&store, Some(id)).is_ok())
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, ok)| *ok).count(), 1);
    let value: Value = maris::store::read_json(&dir.path().join("control.json")).unwrap();
    assert_eq!(
        value["output"],
        results.iter().find(|(_, ok)| *ok).unwrap().0
    );
    assert_eq!(value["expected_rebind_count"], 0);
}

#[test]
fn queued_selection_cannot_target_a_new_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    store.write_json("runtime.json", &runtime()).unwrap();
    assert!(control::request_output_for_session(&store, "old-session", Some("uid:A")).is_err());
    assert!(!dir.path().join("control.json").exists());
}
