use super::*;

#[test]
fn sound_surface_restores_all_preference_and_global_eq_controls() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    for row in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 14, 15] {
        let listening = crate::listening::load(&store).unwrap();
        let profile = store.load().unwrap();
        adjust_sound(
            &store,
            Some("Fixture Speakers"),
            listening.revision,
            profile.revision,
            row,
            1.0,
        )
        .unwrap();
        assert_eq!(store.load().unwrap().revision, profile.revision);
        assert_eq!(
            crate::listening::load(&store).unwrap().revision,
            // Intensity is already at 100%; enabled and level-match are already on.
            listening.revision + u64::from(![5, 14, 15].contains(&row))
        );
    }
    let listening = crate::listening::load(&store).unwrap();
    assert!(listening.effective("Fixture Speakers").bass_assist.enabled);
    assert!(listening.effective("Fixture Speakers").compressor.enabled);
    for row in [10, 11, 12, 13, 16, 25] {
        let state = store.load().unwrap();
        adjust_sound(
            &store,
            Some("Fixture Speakers"),
            listening.revision,
            state.revision,
            row,
            -1.0,
        )
        .unwrap();
        // An already-off bypass or zero crossfeed must not consume useful undo.
        assert_eq!(
            store.load().unwrap().revision,
            state.revision + u64::from(![11, 12].contains(&row))
        );
        assert_eq!(
            crate::listening::load(&store).unwrap().revision,
            listening.revision
        );
    }
    assert_eq!(store.load().unwrap().profile.bands[9].gain_db, -0.5);
}

#[test]
fn unchanged_global_controls_do_not_rename_the_curve_or_consume_history() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    for row in [10, 11, 12] {
        let state = store.load().unwrap();
        let direction = if row == 10 { 1.0 } else { -1.0 };
        assert_eq!(
            adjust_sound(&store, Some("Fixture"), 0, state.revision, row, direction).unwrap(),
            None
        );
        assert_eq!(store.load().unwrap().revision, state.revision);
        assert_eq!(store.load().unwrap().profile, state.profile);
    }
}

#[test]
fn a_no_op_control_does_not_redirect_undo_to_the_wrong_library() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let eq_change = adjust_sound(&store, Some("Speakers"), 0, 0, EQ_ROW_START, 1.0).unwrap();
    assert_eq!(eq_change, Some(UndoTarget::Profile));
    let mut undo = eq_change.unwrap();
    // The same return value feeds pointer, home and inspector key dispatch.
    if let Some(target) = adjust_sound(&store, Some("Speakers"), 0, 1, 5, 1.0).unwrap() {
        undo = target;
    }
    assert_eq!(undo, UndoTarget::Profile);
    assert!(!store.directory.join("listening.json").exists());
    store.undo(Some(1)).unwrap();
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 0.0);
}

#[test]
fn global_controls_reject_invalid_direction_before_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    for row in [10, 11, 14, 15, 16] {
        for direction in [0.0, f64::NAN, f64::INFINITY, 50.0] {
            assert!(adjust_sound(&store, Some("Fixture"), 0, 0, row, direction).is_err());
        }
    }
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(crate::listening::load(&store).unwrap().revision, 0);
}

#[test]
fn settings_entry_stages_first_band_and_requires_explicit_apply() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut workspace = Workspace::Now;
    let mut cursor = 0;
    let mut memory = CursorMemory::default();
    let destination = Workspace::direct(KeyCode::Char('e')).unwrap();
    navigate(&mut workspace, &mut cursor, &mut memory, destination);
    assert_eq!(cursor, EQ_ROW_START);
    assert_eq!(store.load().unwrap().revision, 0);
    let mut runtime = serde_json::json!({
        "active":true, "session_id":"offline-settings-test", "output":"Fixture",
        "profile_key":"Fixture", "sample_rate":48000,
        "device_capability":crate::device_profile::Capability::default(),
        "updated_at_ms":crate::analysis::now_ms()
    });
    store.write_json("runtime.json", &runtime).unwrap();
    let eq = store.load().unwrap();
    let library = crate::listening::load(&store).unwrap();
    let mut editor = crate::configuration::Editor::default();
    renew_settings_telemetry(&store, &mut runtime);
    editor
        .handle(
            &store,
            crate::configuration::Context {
                runtime: &runtime,
                eq: &eq,
                listening: &library,
            },
            &mut cursor,
            KeyCode::Char('+'),
        )
        .unwrap();
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 0.0);
    assert!(editor.pending());
    renew_settings_telemetry(&store, &mut runtime);
    editor
        .handle(
            &store,
            crate::configuration::Context {
                runtime: &runtime,
                eq: &eq,
                listening: &library,
            },
            &mut cursor,
            KeyCode::Enter,
        )
        .unwrap();
    let profile = store.load().unwrap();
    assert_eq!(profile.profile.bands[0].gain_db, 0.5);
    assert_eq!(profile.revision, 1);
    assert_eq!(crate::listening::load(&store).unwrap().revision, 0);
    assert_eq!(
        crate::listening::load(&store)
            .unwrap()
            .effective("Fixture")
            .bass_db,
        0.0
    );
    store.undo(Some(profile.revision)).unwrap();
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 0.0);
}

fn renew_settings_telemetry(store: &Store, displayed: &mut serde_json::Value) {
    let mut current: serde_json::Value =
        crate::store::read_json(&store.directory.join("runtime.json")).unwrap();
    let now = serde_json::json!(crate::analysis::now_ms());
    current["updated_at_ms"] = now.clone();
    displayed["updated_at_ms"] = now;
    store.write_json("runtime.json", &current).unwrap();
}

#[test]
fn expired_settings_cannot_apply_and_recovery_requires_an_explicit_new_draft() {
    let clock = crate::analysis::test_clock::Clock::freeze();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut runtime = serde_json::json!({
        "active":true, "session_id":"offline-settings-aging", "output":"Fixture",
        "profile_key":"Fixture", "sample_rate":48000,
        "device_capability":crate::device_profile::Capability::default(),
        "updated_at_ms":crate::analysis::now_ms()
    });
    store.write_json("runtime.json", &runtime).unwrap();
    let eq = store.load().unwrap();
    let library = crate::listening::load(&store).unwrap();
    let mut editor = crate::configuration::Editor::default();
    let mut cursor = EQ_ROW_START;
    renew_settings_telemetry(&store, &mut runtime);
    editor
        .handle(
            &store,
            crate::configuration::Context {
                runtime: &runtime,
                eq: &eq,
                listening: &library,
            },
            &mut cursor,
            KeyCode::Char('+'),
        )
        .unwrap();
    clock.advance(4_000);
    assert!(editor
        .handle(
            &store,
            crate::configuration::Context {
                runtime: &runtime,
                eq: &eq,
                listening: &library,
            },
            &mut cursor,
            KeyCode::Enter
        )
        .is_err());
    assert!(editor.invalidated());
    assert_eq!(store.load().unwrap().revision, 0);
    let pinned = runtime.clone();
    renew_settings_telemetry(&store, &mut runtime);
    let mut refreshed = runtime.clone();
    refreshed["updated_at_ms"] = pinned["updated_at_ms"].clone();
    assert_eq!(refreshed, pinned);
    assert!(
        editor
            .handle(
                &store,
                crate::configuration::Context {
                    runtime: &runtime,
                    eq: &eq,
                    listening: &library,
                },
                &mut cursor,
                KeyCode::Enter
            )
            .is_err(),
        "renewal cannot revive an invalidated draft"
    );
    editor.cancel();
    for key in [KeyCode::Char('+'), KeyCode::Enter] {
        renew_settings_telemetry(&store, &mut runtime);
        editor
            .handle(
                &store,
                crate::configuration::Context {
                    runtime: &runtime,
                    eq: &eq,
                    listening: &library,
                },
                &mut cursor,
                key,
            )
            .unwrap();
    }
    assert_eq!(store.load().unwrap().revision, 1);
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 0.5);
    assert_eq!(crate::listening::load(&store).unwrap().revision, 0);
    assert!(!store.directory.join("control.json").exists());
}

#[test]
fn direct_inspector_navigation_restores_cursors_without_writing_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut workspace = Workspace::Now;
    let mut cursor = 7;
    let mut memory = CursorMemory::default();
    navigate(&mut workspace, &mut cursor, &mut memory, Workspace::Sound);
    cursor = EQ_ROW_START + 8;
    navigate(&mut workspace, &mut cursor, &mut memory, Workspace::Device);
    navigate(
        &mut workspace,
        &mut cursor,
        &mut memory,
        Workspace::Intelligence,
    );
    navigate(&mut workspace, &mut cursor, &mut memory, Workspace::Sound);
    assert_eq!(cursor, EQ_ROW_START + 8);
    navigate(&mut workspace, &mut cursor, &mut memory, Workspace::Now);
    assert_eq!(cursor, 7);
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(crate::listening::load(&store).unwrap().revision, 0);
}

#[test]
fn invalid_row_and_stale_revision_leave_both_libraries_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    assert!(adjust_sound(&store, Some("Fixture"), 0, 0, usize::MAX, 1.0).is_err());
    assert!(adjust_sound(&store, Some("Fixture"), 99, 0, 0, 1.0).is_err());
    assert!(adjust_sound(&store, Some("Fixture"), 0, 99, 16, 1.0).is_err());
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(crate::listening::load(&store).unwrap().revision, 0);
}

#[test]
fn preset_picker_prefers_the_last_applied_layer_when_both_scene_and_eq_match() {
    let presets = crate::presets::console_catalog();
    let snapshot = crate::store::Snapshot::default();
    let capability = crate::device_profile::Capability::default();
    let scene_index = presets
        .iter()
        .position(|preset| preset.id == "scene:dialogue")
        .unwrap();
    let eq_index = presets
        .iter()
        .position(|preset| preset.id == "flat")
        .unwrap();
    let music = crate::scenes::prepare(
        &crate::music::MusicProfile::default(),
        &capability,
        "scene:dialogue",
    )
    .unwrap()
    .profile;
    let runtime = serde_json::json!({
        "sample_rate": 48_000,
        "device_capability": capability
    });

    assert_eq!(
        preset_picker::preferred_choice(&presets, scene_index, &snapshot, &music, &runtime),
        Some(scene_index)
    );
    assert_eq!(
        preset_picker::preferred_choice(&presets, eq_index, &snapshot, &music, &runtime),
        Some(eq_index),
        "the picker should remember whether the user last applied the scene or EQ layer"
    );
}

#[test]
fn preset_picker_drops_a_preferred_row_after_that_layer_is_manually_changed() {
    let presets = crate::presets::console_catalog();
    let mut snapshot = crate::store::Snapshot::default();
    let music = crate::music::MusicProfile::default();
    let warm_index = presets
        .iter()
        .position(|preset| preset.id == "warm")
        .unwrap();
    snapshot.profile = crate::presets::profile("warm", 48_000).unwrap();
    let runtime = serde_json::json!({
        "sample_rate": 48_000,
        "device_capability": crate::device_profile::Capability::default()
    });
    assert_eq!(
        preset_picker::preferred_choice(&presets, warm_index, &snapshot, &music, &runtime),
        Some(warm_index)
    );

    snapshot.profile.bands[0].gain_db += 0.5;
    snapshot.profile.name = "custom".into();
    assert_ne!(
        preset_picker::preferred_choice(&presets, warm_index, &snapshot, &music, &runtime),
        Some(warm_index)
    );
}

#[test]
fn clean_app_scope_tracks_runtime_but_a_local_draft_is_not_overwritten() {
    let runtime = serde_json::json!({
        "active": true,
        "captured_application_pids": [222, 111, 222]
    });
    let mut pending = vec![999];
    sync_application_scope(&runtime, &mut pending, false);
    assert_eq!(pending, vec![222, 111, 222]);

    pending = vec![333];
    sync_application_scope(&runtime, &mut pending, true);
    assert_eq!(pending, vec![333]);

    sync_application_scope(&serde_json::json!({"active":false}), &mut pending, false);
    assert!(pending.is_empty());
}

#[test]
fn leaving_apps_discards_an_unapplied_scope_and_restores_the_live_capture() {
    let runtime = serde_json::json!({
        "active": true,
        "captured_application_pids": [111, 222]
    });
    let mut pending = vec![333];
    let mut dirty = true;
    discard_application_draft(
        Workspace::Apps,
        Workspace::Now,
        &runtime,
        &mut pending,
        &mut dirty,
    );
    assert_eq!(pending, vec![111, 222]);
    assert!(!dirty);

    pending = vec![333];
    dirty = true;
    discard_application_draft(
        Workspace::Apps,
        Workspace::Apps,
        &runtime,
        &mut pending,
        &mut dirty,
    );
    assert_eq!(pending, vec![333]);
    assert!(
        dirty,
        "staying on Apps must not discard the pending selection"
    );
}
