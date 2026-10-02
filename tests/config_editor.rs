//! Real configuration event reducer against isolated state, never live audio.
use crossterm::event::KeyCode;
use maris::{
    analysis, audio,
    configuration::{Context, Editor, Outcome},
    control_panel::UndoTarget,
    device_profile::Capability,
    listening,
    store::Store,
};
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    store
        .write_json(
            "runtime.json",
            &json!({
                "active":true,"session_id":"editor-fixture","profile_key":"Headphones",
                "output":"Headphones","sample_rate":48000,"music_processing":true,
                "device_identity":{"stable_id":"fixture-uid"},"rebind_count":0,
                "device_binding_revision":1,"output_mode":"pinned",
                "updated_at_ms":analysis::now_ms(),"device_capability":Capability::default()
            }),
        )
        .unwrap();
    (dir, store)
}
fn press(
    editor: &mut Editor,
    store: &Store,
    row: &mut usize,
    key: KeyCode,
) -> anyhow::Result<Outcome> {
    let eq = store.load()?;
    let library = listening::load(store)?;
    let mut runtime = audio::runtime_status(store);
    runtime["device_capability"] =
        serde_json::to_value(maris::device_profile::effective(store, "Headphones")?)?;
    editor.refresh(Context {
        runtime: &runtime,
        eq: &eq,
        listening: &library,
    });
    editor.handle(
        store,
        Context {
            runtime: &runtime,
            eq: &eq,
            listening: &library,
        },
        row,
        key,
    )
}
fn renew_telemetry(store: &Store) {
    let mut runtime: Value =
        maris::store::read_json(&store.directory.join("runtime.json")).unwrap();
    runtime["updated_at_ms"] = json!(analysis::now_ms());
    store.write_json("runtime.json", &runtime).unwrap();
}

#[test]
fn editing_width_does_not_rewrite_unrelated_stored_tone_or_effect_preferences() {
    let (_dir, store) = fixture();
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.bass_db = 6.0;
        p.bass_assist.enabled = true;
        p.bass_assist.amount = 0.4;
        Ok(())
    })
    .unwrap();
    let before = listening::load(&store)
        .unwrap()
        .effective("Headphones")
        .clone();
    let mut editor = Editor::default();
    let mut row = 7;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap();
    let mut expected = before;
    expected.width = 1.05;
    assert_eq!(
        listening::load(&store).unwrap().effective("Headphones"),
        &expected
    );
}

#[test]
fn delayed_apply_uses_current_telemetry_without_rebinding_the_draft() {
    // Both transaction domains must survive normal human editing time. These are
    // isolated state files: no capture, output change or audio process is started.
    let mut cases: Vec<_> = [0, 16]
        .into_iter()
        .map(|row| {
            let (directory, store) = fixture();
            let mut editor = Editor::default();
            let mut selected = row;
            press(&mut editor, &store, &mut selected, KeyCode::Char('+')).unwrap();
            (directory, store, editor, selected)
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    for (_, store, editor, row) in &mut cases {
        renew_telemetry(store);
        let target = if *row == 0 {
            UndoTarget::Listening
        } else {
            UndoTarget::Profile
        };
        assert_eq!(
            press(editor, store, row, KeyCode::Enter).unwrap(),
            Outcome::Applied(Some(target))
        );
        assert!(!editor.pending());
        assert_eq!(
            store.load().unwrap().revision + listening::load(store).unwrap().revision,
            1
        );
    }
}

#[test]
fn delayed_adjustment_keeps_the_original_draft_with_fresh_live_telemetry() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    renew_telemetry(&store);
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap(),
        Outcome::Staged
    );
    no_settings(&store);
    press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap();
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 1.0);
}

fn no_settings(store: &Store) {
    for name in [
        "profile.json",
        "listening.json",
        "listening-previous.json",
        "control.json",
    ] {
        assert!(
            !store.directory.join(name).exists(),
            "unexpected write to {name}"
        );
    }
}

#[test]
fn every_arrow_and_space_browse_without_changing_audio_or_preferences() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 16;
    for key in [
        KeyCode::Right,
        KeyCode::Left,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Char(' '),
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Enter,
    ] {
        for _ in 0..40 {
            assert_eq!(
                press(&mut editor, &store, &mut row, key).unwrap(),
                Outcome::Browse
            );
        }
    }
    assert!(!editor.pending());
    no_settings(&store);
}

#[test]
fn several_eq_edits_stage_without_writes_and_apply_as_one_undoable_transaction() {
    let (_dir, store) = fixture();
    let baseline = store.load().unwrap().profile;
    let mut editor = Editor::default();
    let mut row = 16;
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap(),
        Outcome::Staged
    );
    assert!(editor.pending());
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    press(&mut editor, &store, &mut row, KeyCode::Down).unwrap();
    assert_eq!(row, 17);
    press(&mut editor, &store, &mut row, KeyCode::Char('-')).unwrap();
    no_settings(&store);
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap(),
        Outcome::Applied(Some(UndoTarget::Profile))
    );
    let applied = store.load().unwrap();
    assert_eq!(applied.revision, 1);
    assert_eq!(applied.profile.bands[0].gain_db, 1.0);
    assert_eq!(applied.profile.bands[1].gain_db, -0.5);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap(),
        Outcome::Browse
    );
    assert_eq!(store.load().unwrap().revision, 1);
    assert_eq!(store.undo(Some(1)).unwrap().profile, baseline);
}

#[test]
fn cancel_and_return_to_baseline_do_not_create_history_or_revisions() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Esc).unwrap(),
        Outcome::Cancelled
    );
    no_settings(&store);
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    press(&mut editor, &store, &mut row, KeyCode::Char('-')).unwrap();
    assert!(!editor.pending());
    press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap();
    no_settings(&store);
}

#[test]
fn device_preference_drafts_preserve_correction_and_other_outputs() {
    let (_dir, store) = fixture();
    let library = listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.correction_source = Some("Fixture correction".into());
        p.correction_preamp_db = -3.0;
        p.highpass_hz = Some(45.0);
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(store.directory.join("listening.json")).unwrap();
    let mut editor = Editor::default();
    let mut row = 0;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    row = 3;
    press(&mut editor, &store, &mut row, KeyCode::Char('-')).unwrap();
    assert_eq!(
        std::fs::read(store.directory.join("listening.json")).unwrap(),
        before
    );
    assert_eq!(
        press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap(),
        Outcome::Applied(Some(UndoTarget::Listening))
    );
    let after = listening::load(&store).unwrap();
    assert_eq!(after.revision, 2);
    assert_eq!(after.effective("Headphones").bass_db, 0.5);
    assert_eq!(after.effective("Headphones").air_db, -0.5);
    assert_eq!(
        after.effective("Headphones").correction_source,
        library.effective("Headphones").correction_source
    );
    assert_eq!(after.effective("Headphones").highpass_hz, Some(45.0));
    assert_eq!(after.effective("Other"), library.effective("Other"));
    listening::undo_device(&store, 2, "Headphones").unwrap();
    assert_eq!(
        listening::load(&store).unwrap().effective("Headphones"),
        library.effective("Headphones")
    );
    assert_eq!(store.load().unwrap().revision, 0);
}

#[test]
fn pending_draft_blocks_routing_presets_comparison_and_cross_domain_edits() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    for key in [
        KeyCode::Char('o'),
        KeyCode::Char('p'),
        KeyCode::Char('b'),
        KeyCode::Char('u'),
        KeyCode::Char('q'),
        KeyCode::Tab,
        KeyCode::Char('m'),
    ] {
        assert!(!editor.permits(key));
    }
    assert!(editor.permits(KeyCode::Char('s')));
    assert!(editor.permits(KeyCode::Esc));
    row = 0;
    assert!(press(&mut editor, &store, &mut row, KeyCode::Char('+')).is_err());
    no_settings(&store);
    row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap();
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, 0.5);
}

#[test]
fn changed_route_or_stale_telemetry_invalidates_draft_without_silent_rebase() {
    for field in [
        "session_id",
        "profile_key",
        "output",
        "sample_rate",
        "device_identity",
        "device_binding_revision",
        "rebind_count",
        "updated_at_ms",
    ] {
        let (_dir, store) = fixture();
        let mut editor = Editor::default();
        let mut row = 16;
        press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
        let original: Value =
            maris::store::read_json(&store.directory.join("runtime.json")).unwrap();
        let mut changed = original.clone();
        changed[field] = if field == "updated_at_ms" {
            json!(0)
        } else {
            json!("changed")
        };
        store.write_json("runtime.json", &changed).unwrap();
        assert!(
            press(&mut editor, &store, &mut row, KeyCode::Enter).is_err(),
            "missed {field}"
        );
        assert!(editor.invalidated());
        store.write_json("runtime.json", &original).unwrap();
        assert!(press(&mut editor, &store, &mut row, KeyCode::Enter).is_err());
        no_settings(&store);
    }
}

#[test]
fn concurrent_configuration_changes_are_kept_and_old_draft_cannot_overwrite_them() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
    store
        .edit(Some(0), |p| {
            p.preamp_db = -4.0;
            Ok(())
        })
        .unwrap();
    let updated = std::fs::read(store.directory.join("profile.json")).unwrap();
    assert!(press(&mut editor, &store, &mut row, KeyCode::Enter).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("profile.json")).unwrap(),
        updated
    );
}

#[test]
fn band_q_draft_uses_the_visible_sample_rate_conversion_and_remains_undoable() {
    let (_dir, store) = fixture();
    let before = maris::presets::profile("eqmac:acoustic", 48000).unwrap();
    let base = store
        .edit(Some(0), |p| {
            *p = before.clone();
            Ok(())
        })
        .unwrap();
    // The editor starts from persisted JSON, not the pre-serialization floating-point object.
    let before = store.load().unwrap().profile;
    let visible_q = before.bands[0].q_at(48000);
    let mut editor = Editor::default();
    let mut row = 16;
    press(&mut editor, &store, &mut row, KeyCode::Char(']')).unwrap();
    assert_eq!(store.load().unwrap().profile, before);
    press(&mut editor, &store, &mut row, KeyCode::Enter).unwrap();
    let current = store.load().unwrap();
    assert!((current.profile.bands[0].q - (visible_q + 0.1).clamp(0.2, 5.0)).abs() < 1e-9);
    assert_eq!(current.profile.bands[0].bandwidth_octaves, None);
    assert_eq!(current.revision, base.revision + 1);
    assert_eq!(store.undo(Some(current.revision)).unwrap().profile, before);
}

#[test]
fn capability_changes_invalidate_pending_edits_without_writing_other_parameters() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = 0;
    press(&mut editor, &store, &mut row, KeyCode::Char('+')).unwrap();
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
    assert!(press(&mut editor, &store, &mut row, KeyCode::Enter).is_err());
    assert!(editor.invalidated());
    no_settings(&store);
}

#[test]
fn unknown_or_unsupported_adjustments_do_not_leave_a_partial_draft() {
    let (_dir, store) = fixture();
    let mut editor = Editor::default();
    let mut row = usize::MAX;
    assert!(press(&mut editor, &store, &mut row, KeyCode::Char('+')).is_err());
    row = 1;
    assert!(press(&mut editor, &store, &mut row, KeyCode::Char('+')).is_err());
    assert!(!editor.pending());
    no_settings(&store);
}
