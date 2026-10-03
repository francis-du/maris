use super::*;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    runtime(&store, |_| {});
    (dir, store)
}
fn runtime(store: &Store, change: impl FnOnce(&mut Value)) {
    let mut value = json!({
        "active": true, "music_processing": true, "session_id": "menu-session", "pid": 100,
        "output": "Headphones", "profile_key": "Headphones", "sample_rate": 48000,
        "system_backend": "coreaudio_process_tap", "output_mode": "follow_system_default",
        "device_identity": {"stable_id":"headphones-a"}, "device_binding_revision": 1,
        "rebind_count": 0, "updated_at_ms": crate::analysis::now_ms(),
        "applied_revision": 0, "applied_music_revision": 0
    });
    change(&mut value);
    store.write_json("runtime.json", &value).unwrap();
}
fn heartbeat(store: &Store) {
    // Model the continuing audio writer between deliberate fresh menu actions.
    // Preserve every session, device and revision field; stale-state tests stay explicit.
    let mut value: Value = crate::store::read_json(&store.directory.join("runtime.json")).unwrap();
    value["updated_at_ms"] = json!(crate::analysis::now_ms());
    store.write_json("runtime.json", &value).unwrap();
}
fn inventory() -> Vec<DeviceInfo> {
    ["headphones-a", "headphones-b"]
        .into_iter()
        .map(|uid| DeviceInfo {
            id: format!("uid:{uid}"),
            name: "Headphones".into(),
            direction: "output".into(),
            is_default: uid == "headphones-a",
        })
        .collect()
}

#[test]
fn native_output_selectors_work_without_assuming_a_macos_uid() {
    // This case models a continuing audio writer, independent of runner scheduling.
    let _clock = crate::analysis::test_clock::Clock::freeze();
    for (backend, selector) in [
        ("coreaudio_process_tap", "uid:headphones-b"),
        (
            "pulse_server",
            "pulse:alsa_output.usb-headphones.analog-stereo",
        ),
        (
            "wasapi_process_loopback",
            "wasapi:{0.0.0.00000000}.{fixture}",
        ),
    ] {
        let (_dir, store) = fixture();
        runtime(&store, |r| r["system_backend"] = json!(backend));
        let inventory = vec![DeviceInfo {
            id: selector.into(),
            name: "Chosen headphones".into(),
            direction: "output".into(),
            is_default: false,
        }];
        let mut controller = Controller::default();
        heartbeat(&store);
        controller
            .select_output(&store, Some(selector), &inventory)
            .unwrap();
        assert!(!store.directory.join("control.json").exists());
        heartbeat(&store);
        controller.apply(&store, &inventory).unwrap();
        let command: Value =
            crate::store::read_json(&store.directory.join("control.json")).unwrap();
        assert_eq!(command["output"], selector);
    }
}

#[test]
fn native_output_items_enable_the_current_backend_and_check_only_the_pinned_device() {
    // This case models a continuing audio writer, independent of runner scheduling.
    let _clock = crate::analysis::test_clock::Clock::freeze();
    for (backend, prefix) in [
        ("coreaudio_process_tap", "uid:"),
        ("pulse_server", "pulse:"),
        ("wasapi_process_loopback", "wasapi:"),
    ] {
        let (_dir, store) = fixture();
        runtime(&store, |r| {
            r["system_backend"] = json!(backend);
            r["output_mode"] = json!("pinned");
        });
        heartbeat(&store);
        let state = audio::runtime_status(&store);
        let summary = Summary::read(&store, &state, crate::analysis::now_ms()).unwrap();
        let mut device = inventory()[0].clone();
        device.id = format!("{prefix}headphones-a");
        assert_eq!(
            output_item_state(&summary, &state, &device),
            (true, true),
            "{backend}"
        );
        device.id = format!("{prefix}headphones-b");
        assert_eq!(output_item_state(&summary, &state, &device), (true, false));
        for invalid in ["output:0", "uid:", "pulse:", "wasapi:", "uid:bad\nname"] {
            device.id = invalid.into();
            assert_eq!(
                output_item_state(&summary, &state, &device),
                (false, false),
                "{invalid:?}"
            );
        }
        device.id = format!("{prefix}headphones-a");
        device.direction = "input".into();
        assert_eq!(output_item_state(&summary, &state, &device), (false, false));
        device.direction = "output".into();
        let mut stopped = state.clone();
        stopped["active"] = json!(false);
        let summary = Summary::read(&store, &stopped, crate::analysis::now_ms()).unwrap();
        assert_eq!(
            output_item_state(&summary, &stopped, &device),
            (false, false)
        );
    }
}

#[test]
fn selecting_from_an_outdated_menu_cannot_target_a_new_output() {
    let (_dir, store) = fixture();
    heartbeat(&store);
    let state = audio::runtime_status(&store);
    let mut controller = Controller::default();
    controller.observe(&Summary::read(&store, &state, crate::analysis::now_ms()).unwrap());
    runtime(&store, |r| {
        r["profile_key"] = json!("Other speakers");
        r["rebind_count"] = json!(1);
    });
    heartbeat(&store);
    assert!(controller.select_preset(&store, "focus").is_err());
    heartbeat(&store);
    assert!(controller
        .select_output(&store, Some("uid:headphones-a"), &inventory())
        .is_err());
    assert!(!controller.has_pending());
    assert!(!store.directory.join("control.json").exists());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

#[test]
fn native_menu_rejects_index_selectors_and_mismatched_backend_ids() {
    let (_dir, store) = fixture();
    for selector in [
        "output:0",
        "input:0",
        "pulse:sink",
        "wasapi:endpoint",
        "uid:",
        "uid:bad\nname",
    ] {
        let inventory = vec![DeviceInfo {
            id: selector.into(),
            name: "Headphones".into(),
            direction: "output".into(),
            is_default: false,
        }];
        heartbeat(&store);
        assert!(
            Controller::default()
                .select_output(&store, Some(selector), &inventory)
                .is_err(),
            "accepted {selector:?}"
        );
    }
    assert!(!store.directory.join("control.json").exists());
}

#[test]
fn selecting_and_cancelling_a_scene_never_changes_audio_or_preferences() {
    let (_dir, store) = fixture();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller.select_preset(&store, "night-dialogue").unwrap();
    assert!(controller.has_pending());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!store.directory.join("control.json").exists());
    assert!(!store.directory.join("listening.json").exists());
    assert!(controller
        .preview_lines()
        .iter()
        .any(|line| line == "Compression enabled without makeup gain"));
    controller.cancel();
    assert!(!controller.has_pending());
    heartbeat(&store);
    assert!(controller.apply(&store, &[]).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

#[test]
fn applied_scene_is_device_scoped_and_undo_survives_ab_comparison() {
    let (_dir, store) = fixture();
    let before = MusicProfile {
        correction_source: Some("fixture measurement".into()),
        correction_preamp_db: -3.0,
        highpass_hz: Some(35.0),
        balance: 0.1,
        ..MusicProfile::default()
    };
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        *p = before.clone();
        Ok(())
    })
    .unwrap();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller.select_preset(&store, "night-dialogue").unwrap();
    heartbeat(&store);
    controller.apply(&store, &[]).unwrap();
    let changed = listening::load(&store).unwrap();
    assert_eq!(changed.revision, 2);
    assert!(changed.effective("Headphones").compressor.enabled);
    assert_eq!(
        changed.effective("Headphones").correction_source,
        before.correction_source
    );
    assert_eq!(changed.effective("Headphones").balance, before.balance);
    assert_eq!(changed.effective("Unrelated"), &MusicProfile::default());
    assert_eq!(store.load().unwrap().revision, 0);
    let history = std::fs::read(store.directory.join("listening-previous.json")).unwrap();
    heartbeat(&store);
    controller.compare(&store).unwrap();
    heartbeat(&store);
    controller.compare(&store).unwrap();
    assert_eq!(
        std::fs::read(store.directory.join("listening-previous.json")).unwrap(),
        history
    );
    heartbeat(&store);
    controller.undo(&store).unwrap();
    let restored = listening::load(&store).unwrap();
    assert_eq!(restored.revision, 5);
    assert_eq!(restored.effective("Headphones"), &before);
}

#[test]
fn preview_is_rejected_after_any_route_identity_or_revision_change() {
    for field in [
        "session_id",
        "profile_key",
        "output",
        "sample_rate",
        "rebind_count",
        "device_binding_revision",
        "device_identity",
    ] {
        let (_dir, store) = fixture();
        let mut controller = Controller::default();
        heartbeat(&store);
        controller.select_preset(&store, "focus").unwrap();
        runtime(&store, |value| {
            value[field] = match field {
                "sample_rate" => json!(96000),
                "rebind_count" => json!(1),
                "device_binding_revision" => json!(2),
                "device_identity" => json!({"stable_id":"other"}),
                _ => json!("other"),
            }
        });
        heartbeat(&store);
        assert!(
            controller.apply(&store, &[]).is_err(),
            "unexpected apply after {field}"
        );
        assert_eq!(listening::load(&store).unwrap().revision, 0);
    }
    let (_dir, store) = fixture();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller.select_preset(&store, "focus").unwrap();
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.air_db = 0.2;
        Ok(())
    })
    .unwrap();
    heartbeat(&store);
    assert!(controller.apply(&store, &[]).is_err());
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("Headphones")
            .air_db,
        0.2
    );
}

#[test]
fn previews_expire_and_do_not_accept_future_timestamps() {
    for age in [PREVIEW_TTL_MS + 1, u64::MAX] {
        let (_dir, store) = fixture();
        let mut controller = Controller::default();
        heartbeat(&store);
        controller.select_preset(&store, "focus").unwrap();
        controller.pending.as_mut().unwrap().guard.created_at_ms = if age == u64::MAX {
            age
        } else {
            crate::analysis::now_ms() - age
        };
        heartbeat(&store);
        assert!(controller.apply(&store, &[]).is_err());
        assert_eq!(listening::load(&store).unwrap().revision, 0);
    }
}

#[test]
fn capability_and_eq_changes_invalidate_scene_selection() {
    let _clock = crate::analysis::test_clock::Clock::freeze();
    let (_dir, store) = fixture();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller.select_preset(&store, "focus").unwrap();
    store
        .edit(Some(0), |p| {
            p.bands[0].gain_db = -1.0;
            Ok(())
        })
        .unwrap();
    heartbeat(&store);
    assert!(controller.apply(&store, &[]).is_err());
    heartbeat(&store);
    controller.select_preset(&store, "focus").unwrap();
    let cap = Capability {
        max_preference_boost_db: 0.0,
        ..Capability::default()
    };
    store
        .write_json(
            "device-capabilities.json",
            &json!({"revision":1,"devices":{"Headphones":cap}}),
        )
        .unwrap();
    heartbeat(&store);
    assert!(controller.apply(&store, &[]).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

#[test]
fn output_selection_is_uid_bound_and_only_apply_queues_a_command() {
    let (_dir, store) = fixture();
    let devices = inventory();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller
        .select_output(&store, Some(&devices[1].id), &devices)
        .unwrap();
    assert!(!store.directory.join("control.json").exists());
    let mut reordered = devices.clone();
    reordered.reverse();
    heartbeat(&store);
    controller.apply(&store, &reordered).unwrap();
    let command: Value = crate::store::read_json(&store.directory.join("control.json")).unwrap();
    assert_eq!(command["output"], "uid:headphones-b");
    assert_eq!(command["session_id"], "menu-session");
    assert!(!controller.has_pending());
    heartbeat(&store);
    assert!(controller.apply(&store, &reordered).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

#[test]
fn vanished_or_ambiguous_output_does_not_fall_back_or_leave_old_selection_armed() {
    let (_dir, store) = fixture();
    let devices = inventory();
    let mut controller = Controller::default();
    heartbeat(&store);
    controller
        .select_output(&store, Some(&devices[0].id), &devices)
        .unwrap();
    heartbeat(&store);
    assert!(controller.apply(&store, &devices[1..]).is_err());
    assert!(!store.directory.join("control.json").exists());
    let duplicates = vec![devices[0].clone(), devices[0].clone()];
    heartbeat(&store);
    assert!(controller
        .select_output(&store, Some(&devices[0].id), &duplicates)
        .is_err());
    assert!(!controller.has_pending());
    heartbeat(&store);
    assert!(controller
        .select_output(&store, Some("Headphones"), &devices)
        .is_err());
}

#[test]
fn missing_stale_or_offline_runtime_cannot_edit_fallback_profile() {
    for missing in [
        "session_id",
        "profile_key",
        "sample_rate",
        "music_processing",
    ] {
        let (_dir, store) = fixture();
        runtime(&store, |value| {
            value.as_object_mut().unwrap().remove(missing);
        });
        let mut controller = Controller::default();
        assert!(controller.select_preset(&store, "focus").is_err());
        assert!(controller.compare(&store).is_err());
        assert_eq!(listening::load(&store).unwrap().revision, 0);
    }
    for active in [false, true] {
        let (_dir, store) = fixture();
        runtime(&store, |value| {
            value["active"] = json!(active);
            value["updated_at_ms"] = json!(crate::analysis::now_ms() - 10000);
        });
        assert!(Controller::default()
            .select_preset(&store, "focus")
            .is_err());
    }
}

#[test]
fn pending_device_limits_are_not_reported_applied_just_because_profile_revisions_match() {
    let (_dir, store) = fixture();
    runtime(&store, |runtime| {
        runtime["settings_pending"] = json!(true);
        runtime["requested_music_revision"] = json!(0);
    });
    heartbeat(&store);
    let mut state = audio::runtime_status(&store);
    let now = crate::analysis::now_ms();
    assert_eq!(
        Summary::read(&store, &state, now).unwrap().status,
        "Pending audio update"
    );
    assert_eq!(
        crate::ui::tui::studio::controls::application_state(&state, 0, now),
        "Pending audio update"
    );
    state["settings_pending"] = json!(false);
    assert_eq!(
        Summary::read(&store, &state, now).unwrap().status,
        "Applied"
    );
    assert_eq!(
        crate::ui::tui::studio::controls::application_state(&state, 0, now),
        "Applied"
    );
    state["settings_pending"] = json!(true);
    state["updated_at_ms"] = json!(0);
    assert_eq!(
        Summary::read(&store, &state, now).unwrap().status,
        "Awaiting telemetry"
    );
    assert_eq!(
        crate::ui::tui::studio::controls::application_state(&state, 0, now),
        "Awaiting telemetry"
    );
}

#[test]
fn summary_distinguishes_listening_scene_global_eq_and_callback_application() {
    // This fixture represents a live session, independent of CI disk/scheduler delays.
    let _clock = crate::analysis::test_clock::Clock::freeze();
    let (_dir, store) = fixture();
    listening::preset(&store, Some(0), Some("Headphones"), "focus").unwrap();
    store
        .edit(Some(0), |p| {
            *p = crate::profile::Profile::preset("warm").unwrap();
            Ok(())
        })
        .unwrap();
    let state = audio::runtime_status(&store);
    let summary = Summary::read(&store, &state, crate::analysis::now_ms()).unwrap();
    assert_eq!(summary.preset_id, Some("focus"));
    assert_eq!(summary.status, "Pending audio update");
    assert!(summary.eq.contains("Warm"));
    runtime(&store, |value| {
        value["applied_revision"] = json!(1);
        value["applied_music_revision"] = json!(1);
    });
    let summary = Summary::read(
        &store,
        &audio::runtime_status(&store),
        crate::analysis::now_ms(),
    )
    .unwrap();
    assert_eq!(summary.status, "Applied");
    listening::edit(&store, Some(1), Some("Headphones"), |p| {
        p.air_db = -0.23;
        Ok(())
    })
    .unwrap();
    let summary = Summary::read(
        &store,
        &audio::runtime_status(&store),
        crate::analysis::now_ms(),
    )
    .unwrap();
    assert_eq!(summary.preset_id, None);
}

#[test]
fn current_device_undo_cannot_revert_another_devices_last_edit() {
    let (_dir, store) = fixture();
    listening::preset(&store, Some(0), Some("Other speakers"), "focus").unwrap();
    let library = listening::load(&store).unwrap();
    assert!(!listening::can_undo_device(&store, &library, "Headphones").unwrap());
    heartbeat(&store);
    assert!(Controller::default().undo(&store).is_err());
    assert_eq!(listening::load(&store).unwrap().devices, library.devices);
}

#[test]
fn immediate_menu_actions_reject_a_new_output_or_a_new_revision_since_rendering() {
    let (_dir, store) = fixture();
    listening::preset(&store, Some(0), Some("Headphones"), "focus").unwrap();
    let mut controller = Controller::default();
    heartbeat(&store);
    let displayed = Summary::read(
        &store,
        &audio::runtime_status(&store),
        crate::analysis::now_ms(),
    )
    .unwrap();
    assert!(displayed.current && displayed.controls_enabled);
    controller.observe(&displayed);
    runtime(&store, |value| {
        value["profile_key"] = json!("Speakers");
        value["rebind_count"] = json!(1);
    });
    heartbeat(&store);
    assert!(controller.compare(&store).is_err());
    heartbeat(&store);
    assert!(controller.undo(&store).is_err());
    heartbeat(&store);
    assert!(controller.toggle_processing(&store).is_err());
    assert_eq!(store.load().unwrap().revision, 0);
    runtime(&store, |_| {});
    listening::edit(&store, Some(1), Some("Headphones"), |profile| {
        profile.bass_db = 0.25;
        Ok(())
    })
    .unwrap();
    heartbeat(&store);
    assert!(controller.compare(&store).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 2);
}

#[test]
fn an_unavailable_rendered_target_blocks_actions_until_a_fresh_summary_is_observed() {
    for condition in ["stale", "stopped", "unsupported"] {
        let (_dir, store) = fixture();
        listening::preset(&store, Some(0), Some("Headphones"), "focus").unwrap();
        runtime(&store, |value| match condition {
            "stale" => {
                value["updated_at_ms"] = json!(crate::analysis::now_ms().saturating_sub(2000));
            }
            "stopped" => value["active"] = json!(false),
            _ => value["music_processing"] = json!(false),
        });
        let unavailable = Summary::read(
            &store,
            &audio::runtime_status(&store),
            crate::analysis::now_ms(),
        )
        .unwrap();
        assert!(!unavailable.controls_enabled);
        let mut controller = Controller::default();
        controller.observe(&unavailable);
        runtime(&store, |value| {
            value["profile_key"] = json!("Speakers");
            value["rebind_count"] = json!(1);
        });
        let previous = listening::load(&store).unwrap();
        heartbeat(&store);
        assert!(controller.compare(&store).is_err(), "{condition}");
        heartbeat(&store);
        assert!(controller.undo(&store).is_err(), "{condition}");
        heartbeat(&store);
        assert!(controller.toggle_processing(&store).is_err(), "{condition}");
        heartbeat(&store);
        assert!(controller.select_preset(&store, "focus").is_err());
        heartbeat(&store);
        assert!(controller
            .select_output(&store, Some("uid:headphones-b"), &inventory())
            .is_err());
        assert!(!controller.has_pending());
        assert_eq!(
            serde_json::to_value(listening::load(&store).unwrap()).unwrap(),
            serde_json::to_value(&previous).unwrap()
        );
        assert_eq!(store.load().unwrap().revision, 0);
        assert!(!store.directory.join("control.json").exists());

        runtime(&store, |_| {});
        let fresh = Summary::read(
            &store,
            &audio::runtime_status(&store),
            crate::analysis::now_ms(),
        )
        .unwrap();
        assert!(fresh.current && fresh.controls_enabled);
        controller.observe(&fresh);
        heartbeat(&store);
        controller.compare(&store).unwrap();
        assert_eq!(
            listening::load(&store).unwrap().revision,
            previous.revision + 1
        );
    }
}

#[test]
fn menu_and_terminal_do_not_report_unapplied_mixer_changes_as_applied() {
    let (_dir, store) = fixture();
    let saved = crate::mixer::edit(&store, Some(0), |config| {
        config.buses[0].gain_db = -2.0;
        Ok(())
    })
    .unwrap();
    runtime(&store, |r| {
        r["system_backend"] = json!("multi_device_mixer");
        r["requested_music_revision"] = json!(0);
        r["mixer"] = json!({"revision":0,"processing_revision":0,"restart_required":false});
        r["mixer_control"] = json!({"revision":saved.revision});
    });
    heartbeat(&store);
    let mut state = audio::runtime_status(&store);
    let now = crate::analysis::now_ms();
    for (applied, restart, expected) in [
        (json!(0), json!(false), "Pending audio update"),
        (json!(saved.revision), json!(false), "Applied"),
        (Value::Null, json!(false), "Apply state unknown"),
        (json!(saved.revision), Value::Null, "Apply state unknown"),
        (
            json!(saved.revision),
            json!(true),
            "Mixer assignments changed; stop and restart before adjusting channels",
        ),
    ] {
        state["mixer"]["processing_revision"] = applied;
        state["mixer"]["restart_required"] = restart;
        assert_eq!(Summary::read(&store, &state, now).unwrap().status, expected);
        assert_eq!(
            crate::ui::tui::studio::controls::application_state(&state, 0, now),
            expected
        );
    }
}

#[test]
fn preset_groups_include_every_shipped_listening_id_exactly_once() {
    let ids: Vec<_> = PRESET_GROUPS
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    assert_eq!(ids.len(), music::PRESETS.len());
    let unique: std::collections::BTreeSet<_> = ids.into_iter().collect();
    assert_eq!(unique, music::PRESETS.into_iter().collect());
    for (heading, _) in PRESET_GROUPS {
        assert!(i18n::has_translation(heading));
    }
}
