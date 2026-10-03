#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use maris::{analysis, control, store::Store};
use serde_json::Value;

fn active_native(store: &Store) {
    store
        .write_json(
            "runtime.json",
            &serde_json::json!({
                "active": true,
                "session_id": "test-session",
                "pid": 9999,
                "system_backend": "coreaudio_process_tap",
                "updated_at_ms": analysis::now_ms()
            }),
        )
        .unwrap();
}

#[test]
fn output_switch_command_pins_and_releases_native_output() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    active_native(&store);

    live_telemetry::refresh(&store);
    control::request_output(&store, Some("Studio Headphones")).unwrap();
    let command: Value = maris::store::read_json(&directory.path().join("control.json")).unwrap();
    assert_eq!(command["session_id"], "test-session");
    assert_eq!(command["action"], "select_output");
    assert_eq!(command["output"], "Studio Headphones");

    live_telemetry::refresh(&store);
    assert!(control::request_output(&store, None).is_err());
    // Simulate the control thread consuming the first command; a second request
    // must not silently replace an unconsumed selection from another UI.
    std::fs::remove_file(directory.path().join("control.json")).unwrap();
    live_telemetry::refresh(&store);
    control::request_output(&store, None).unwrap();
    let command: Value = maris::store::read_json(&directory.path().join("control.json")).unwrap();
    assert_eq!(command["action"], "select_output");
    assert!(command["output"].is_null());
}

#[test]
fn application_switch_command_is_session_bound_and_supports_system_scope() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    active_native(&store);

    live_telemetry::refresh(&store);
    control::request_applications(&store, &[123, 456]).unwrap();
    let command: Value = maris::store::read_json(&directory.path().join("control.json")).unwrap();
    assert_eq!(command["session_id"], "test-session");
    assert_eq!(command["action"], "select_applications");
    assert_eq!(command["pids"], serde_json::json!([123, 456]));

    live_telemetry::refresh(&store);
    assert!(control::request_applications(&store, &[]).is_err());
    std::fs::remove_file(directory.path().join("control.json")).unwrap();
    live_telemetry::refresh(&store);
    control::request_applications(&store, &[]).unwrap();
    let command: Value = maris::store::read_json(&directory.path().join("control.json")).unwrap();
    assert_eq!(command["pids"], serde_json::json!([]));
}

#[test]
fn application_switch_rejects_invalid_or_duplicate_pids() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    active_native(&store);
    live_telemetry::refresh(&store);
    assert!(control::request_applications(&store, &[0]).is_err());
    live_telemetry::refresh(&store);
    assert!(control::request_applications(&store, &[9999]).is_err());
    live_telemetry::refresh(&store);
    assert!(control::request_applications(&store, &[123, 123]).is_err());
    assert!(!directory.path().join("control.json").exists());
}

#[test]
fn invalid_runtime_timestamps_cannot_authorize_audio_changes() {
    for timestamp in [
        serde_json::json!(u64::MAX),
        serde_json::json!(0),
        serde_json::json!("not a timestamp"),
        Value::Null,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path());
        active_native(&store);
        let mut runtime = maris::audio::runtime_status(&store);
        runtime["updated_at_ms"] = timestamp.clone();
        store.write_json("runtime.json", &runtime).unwrap();

        let observed = maris::audio::runtime_status(&store);
        assert_eq!(observed["active"], false, "timestamp {timestamp}");
        assert_eq!(observed["stale"], true, "timestamp {timestamp}");
        assert!(control::request_output(&store, Some("Headphones")).is_err());
        assert!(control::request_applications(&store, &[123]).is_err());
        control::request_stop(&store).unwrap();
        assert!(!directory.path().join("control.json").exists());
    }
}

#[test]
fn output_switch_rejects_non_native_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    store
        .write_json(
            "runtime.json",
            &serde_json::json!({
                "active": true,
                "session_id": "test-session",
                "system_backend": "device_stream",
                "updated_at_ms": analysis::now_ms()
            }),
        )
        .unwrap();
    assert!(control::request_output(&store, Some("Headphones")).is_err());
    assert!(control::request_applications(&store, &[123]).is_err());
    assert!(!directory.path().join("control.json").exists());
}
