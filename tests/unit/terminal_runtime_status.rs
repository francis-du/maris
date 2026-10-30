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

#[test]
fn retained_handoff_is_labeled_as_a_snapshot_without_inventing_restoration() {
    for armed in [json!(true), json!(false), serde_json::Value::Null] {
        let final_status = final_runtime_status(json!({
            "active": true,
            "updated_at_ms": 500,
            "coreaudio_fail_open_reason": "capture_stalled",
            "coreaudio_handoff": {
                "snapshot_phase": "live",
                "replacement_armed": armed,
                "configuration_pending": false,
                "output_callback_ready": true,
                "watchdog_timeout_ms": 2_000
            }
        }));
        assert_eq!(final_status["active"], false);
        let handoff = &final_status["coreaudio_handoff"];
        assert_eq!(handoff["snapshot_phase"], "before_teardown");
        assert_eq!(handoff["replacement_armed"], armed);
        assert_eq!(handoff["watchdog_timeout_ms"], 2_000);
    }
}

#[test]
fn runtime_observation_reads_once_before_sampling_its_clock() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::at(temporary.path());
    store
        .write_json(
            "runtime.json",
            &json!({"active":true,"updated_at_ms":5000,"session_id":"observed"}),
        )
        .unwrap();
    let mut samples = 0;
    let (observed, now) = crate::audio::runtime_status_with_clock(&store, || {
        samples += 1;
        // Publishing at the clock boundary must not replace the snapshot that
        // was already read, or make its valid heartbeat look like the future.
        store
            .write_json(
                "runtime.json",
                &json!({"active":true,"updated_at_ms":5002,"session_id":"next"}),
            )
            .unwrap();
        5001
    });
    assert_eq!(samples, 1);
    assert_eq!(now, 5001);
    assert_eq!(observed["active"], true);
    assert_eq!(observed["updated_at_ms"], 5000);
    assert_eq!(observed["session_id"], "observed");
    assert_eq!(
        crate::audio::runtime_status_at(&store, 5002)["session_id"],
        "next"
    );
}

#[test]
fn fixed_runtime_clock_still_rejects_future_and_expired_heartbeats() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::at(temporary.path());
    for (heartbeat, active) in [(4000, true), (1000, true), (999, false), (4001, false)] {
        store
            .write_json(
                "runtime.json",
                &json!({"active":true,"updated_at_ms":heartbeat}),
            )
            .unwrap();
        let observed = crate::audio::runtime_status_at(&store, 4000);
        assert_eq!(observed["active"], active, "heartbeat {heartbeat}");
        assert_eq!(observed["stale"] == true, !active, "heartbeat {heartbeat}");
    }
}
