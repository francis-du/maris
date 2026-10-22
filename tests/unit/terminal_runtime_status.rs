//! Final runtime-state fixtures never create native streams or request audio permissions.
use super::final_runtime_status;
use crate::control::store::Store;
use serde_json::json;

#[test]
fn every_coreaudio_fail_open_reason_survives_the_final_store_write() {
    for reason in [
        "output_stalled",
        "capture_stalled",
        "output_and_capture_stalled",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path());
        store
            .write_json("runtime.json", &json!({"active":true,"updated_at_ms":100}))
            .unwrap();
        let final_status = final_runtime_status(json!({
            "active": true,
            "updated_at_ms": 200,
            "session_id": "failed-session",
            "system_backend": "coreaudio_process_tap",
            "coreaudio_fail_open_reason": reason,
            "coreaudio_capture_callback_calls": 31,
            "output_callback_calls": 17,
            "output_quarantined": true,
            "audio_callback_errors": 2
        }));
        store.write_json("runtime.json", &final_status).unwrap();

        let observed = crate::audio::runtime_status_at(&store, 200);
        assert_eq!(observed["active"], false);
        assert_eq!(observed["session_id"], "failed-session");
        assert_eq!(observed["coreaudio_fail_open_reason"], reason);
        assert_eq!(observed["coreaudio_capture_callback_calls"], 31);
        assert_eq!(observed["output_callback_calls"], 17);
        assert_eq!(observed["output_quarantined"], true);
        assert_eq!(observed["audio_callback_errors"], 2);
    }
}

#[test]
fn terminal_failure_survives_missing_periodic_snapshot_and_destroyed_tap_state() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::at(temporary.path());
    // Model the final state after the tap was destroyed before any periodic
    // status write: its absence must not erase an already classified reason.
    let final_status = final_runtime_status(json!({
        "active": false,
        "updated_at_ms": 300,
        "session_id": "restore-failed",
        "coreaudio_tap_mode": null,
        "coreaudio_fail_open_reason": "output_and_capture_stalled",
        "output_quarantined": true,
        "audio_callback_errors": 1
    }));
    store.write_json("runtime.json", &final_status).unwrap();

    let observed = crate::audio::runtime_status_at(&store, 3_301);
    assert_eq!(observed["active"], false);
    assert_eq!(observed["stale"], true);
    assert_eq!(observed["coreaudio_tap_mode"], serde_json::Value::Null);
    assert_eq!(
        observed["coreaudio_fail_open_reason"],
        "output_and_capture_stalled"
    );
    assert_eq!(observed["output_quarantined"], true);
    assert_eq!(observed["audio_callback_errors"], 1);
}

#[test]
fn normal_stop_keeps_the_existing_compact_inactive_contract() {
    for reason in [None, Some(serde_json::Value::Null)] {
        let mut status = json!({
            "active": true,
            "updated_at_ms": 400,
            "session_id": "normal-session",
            "output_callback_calls": 50
        });
        if let Some(reason) = reason {
            status["coreaudio_fail_open_reason"] = reason;
        }
        assert_eq!(
            final_runtime_status(status),
            json!({"active":false,"updated_at_ms":400})
        );
    }
}
