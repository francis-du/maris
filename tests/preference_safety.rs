//! Regression cases for actions that must not change an unrelated control or output.
use maris::{
    analysis, listening,
    music::MusicProfile,
    music_view,
    sound_cli::{self, Action},
    store::Store,
};
use serde_json::json;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    (dir, store)
}

fn set_bass(bass: Option<f64>) -> Action {
    Action::Set {
        bass,
        virtual_bass: None,
        presence: None,
        air: None,
        softness: None,
        intensity: None,
        adaptive: None,
        width: None,
        virtual_surround: None,
        stereo_focus: None,
        balance: None,
        compressor: None,
    }
}

#[test]
fn boost_at_device_limit_is_a_real_no_op_without_false_saved_changes() {
    for row in [0, 2, 3] {
        let (_dir, store) = store();
        let before = listening::edit(&store, Some(0), Some("MacBook Speakers"), |p| {
            p.bass_db = 1.5;
            p.presence_db = 1.5;
            p.air_db = 1.5;
            p.enabled = false;
            p.reference = true;
            Ok(())
        })
        .unwrap();
        let history = std::fs::read(store.directory.join("listening-previous.json")).unwrap();
        assert!(
            !music_view::adjust(&store, Some("MacBook Speakers"), before.revision, row, 1.0)
                .unwrap()
        );
        let after = listening::load(&store).unwrap();
        assert_eq!(after.revision, before.revision);
        assert_eq!(
            after.effective("MacBook Speakers"),
            before.effective("MacBook Speakers")
        );
        assert_eq!(
            std::fs::read(store.directory.join("listening-previous.json")).unwrap(),
            history
        );
    }
}

#[test]
fn decrease_from_a_legacy_boost_above_device_limit_changes_the_audible_value() {
    let (_dir, store) = store();
    listening::edit(&store, Some(0), Some("MacBook Speakers"), |p| {
        p.bass_db = 6.0;
        Ok(())
    })
    .unwrap();
    assert!(music_view::adjust(&store, Some("MacBook Speakers"), 1, 0, -1.0).unwrap());
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("MacBook Speakers")
            .bass_db,
        1.0
    );
}

#[test]
fn decrementing_an_off_effect_never_enables_it_or_changes_comparison() {
    for row in [1, 6] {
        let (_dir, store) = store();
        let before = listening::edit(&store, Some(0), Some("Speakers"), |p| {
            p.bass_assist.enabled = false;
            p.bass_assist.amount = 0.35;
            p.adaptive.enabled = false;
            p.adaptive.strength = 0.45;
            p.enabled = false;
            p.reference = true;
            Ok(())
        })
        .unwrap();
        let history = std::fs::read(store.directory.join("listening-previous.json")).unwrap();
        music_view::adjust(&store, Some("Speakers"), before.revision, row, -1.0).unwrap();
        let after = listening::load(&store).unwrap();
        assert_eq!(
            after.effective("Speakers"),
            before.effective("Speakers"),
            "OFF minus changed row {row}"
        );
        assert_eq!(after.revision, before.revision);
        assert_eq!(
            std::fs::read(store.directory.join("listening-previous.json")).unwrap(),
            history
        );
    }
}

#[test]
fn enabling_an_effect_starts_with_one_visible_step_not_a_hidden_cached_amount() {
    for row in [1, 6] {
        let (_dir, store) = store();
        listening::edit(&store, Some(0), Some("Speakers"), |p| {
            p.bass_assist.enabled = false;
            p.bass_assist.amount = 0.8;
            p.adaptive.enabled = false;
            p.adaptive.strength = 0.9;
            Ok(())
        })
        .unwrap();
        music_view::adjust(&store, Some("Speakers"), 1, row, 1.0).unwrap();
        let state = listening::load(&store).unwrap();
        let p = state.effective("Speakers");
        let actual = if row == 1 {
            p.bass_assist.amount
        } else {
            p.adaptive.strength
        };
        assert!((actual - 0.1).abs() < 1e-9, "first step was {actual}");
        assert_eq!(state.revision, 2);
    }
}

#[test]
fn spatial_effect_steps_are_bounded_and_undoable_without_touching_tone() {
    for row in [10, 11] {
        let (_dir, store) = store();
        listening::edit(&store, Some(0), Some("Headphones"), |p| {
            p.bass_db = 1.25;
            p.presence_db = -0.5;
            Ok(())
        })
        .unwrap();
        assert!(music_view::adjust(&store, Some("Headphones"), 1, row, 1.0).unwrap());
        let state = listening::load(&store).unwrap();
        let p = state.effective("Headphones");
        assert_eq!(p.bass_db, 1.25);
        assert_eq!(p.presence_db, -0.5);
        if row == 10 {
            assert!((p.virtual_surround - 0.1).abs() < 1e-9);
        } else {
            assert!((p.stereo_focus - 0.1).abs() < 1e-9);
        }
        let undone = listening::undo_device(&store, state.revision, "Headphones").unwrap();
        assert_eq!(undone.effective("Headphones").virtual_surround, 0.0);
        assert_eq!(undone.effective("Headphones").stereo_focus, 0.0);
    }
}

#[test]
fn unsupported_virtual_bass_rejects_without_enabling_an_inaudible_setting() {
    let (_dir, store) = store();
    assert!(music_view::adjust(&store, Some("Headphones"), 0, 1, 1.0).is_err());
    assert!(!store.directory.join("listening.json").exists());
}

#[test]
fn invalid_adjustment_actions_fail_before_any_preference_write() {
    for (row, direction) in [
        (usize::MAX, 1.0),
        (0, f64::NAN),
        (0, f64::INFINITY),
        (0, 20.0),
        (9, 0.0),
    ] {
        let (_dir, store) = store();
        assert!(music_view::adjust(&store, Some("Speakers"), 0, row, direction).is_err());
        assert!(!store.directory.join("listening.json").exists());
    }
}

#[test]
fn a_control_at_its_limit_does_not_exit_reference_or_enable_processing() {
    let (_dir, store) = store();
    let before = listening::edit(&store, Some(0), Some("Speakers"), |p| {
        p.intensity = 1.0;
        p.enabled = false;
        p.reference = true;
        Ok(())
    })
    .unwrap();
    music_view::adjust(&store, Some("Speakers"), 1, 5, 1.0).unwrap();
    let after = listening::load(&store).unwrap();
    assert_eq!(after.effective("Speakers"), before.effective("Speakers"));
    assert_eq!(after.revision, 1);
}

#[test]
fn a_no_op_on_an_unsaved_output_does_not_materialize_a_profile_or_history() {
    let (_dir, store) = store();
    music_view::adjust(&store, Some("Speakers"), 0, 1, -1.0).unwrap();
    assert!(!store.directory.join("listening.json").exists());
    assert!(!store.directory.join("listening-previous.json").exists());
}

#[test]
fn another_outputs_ab_comparison_does_not_block_or_get_reverted_by_scoped_undo() {
    let (_dir, store) = store();
    let focus = listening::preset(&store, Some(0), Some("Headphones"), "focus").unwrap();
    let compared = listening::compare(&store, Some(1), Some("Speakers"), true).unwrap();
    assert!(listening::can_undo_device(&store, &compared, "Headphones").unwrap());
    let undone = listening::undo_device(&store, 2, "Headphones").unwrap();
    assert_eq!(undone.effective("Headphones"), &MusicProfile::default());
    assert_eq!(undone.effective("Speakers"), compared.effective("Speakers"));
    assert!(undone.effective("Speakers").reference);
    listening::compare(&store, Some(3), Some("Speakers"), false).unwrap();
    let redone = listening::undo_device(&store, 4, "Headphones").unwrap();
    assert_eq!(
        redone.effective("Headphones"),
        focus.effective("Headphones")
    );
    assert!(!redone.effective("Speakers").reference);
}

#[test]
fn implicit_cli_writes_never_fall_back_to_default_when_current_output_is_unknown() {
    for runtime in [
        json!({"active":false}),
        json!({"active":true,"updated_at_ms":analysis::now_ms(),"output":"Headphones"}),
        json!({"active":true,"updated_at_ms":analysis::now_ms().saturating_sub(10000),"profile_key":"Headphones"}),
        json!({"active":true,"updated_at_ms":analysis::now_ms()+60000,"profile_key":"Headphones"}),
    ] {
        let (_dir, store) = store();
        store.write_json("runtime.json", &runtime).unwrap();
        assert!(sound_cli::run(&store, None, None, set_bass(Some(1.0))).is_err());
        assert!(!store.directory.join("listening.json").exists());
        // Read-only discovery stays usable without an audio session.
        assert!(sound_cli::run(&store, None, None, Action::Status).is_ok());
    }
}

#[test]
fn explicit_cli_defaults_and_named_offline_profiles_still_work() {
    let (_dir, store) = store();
    sound_cli::run(&store, Some("default".into()), Some(0), set_bass(Some(0.5))).unwrap();
    sound_cli::run(
        &store,
        Some("Headphones".into()),
        Some(1),
        set_bass(Some(1.0)),
    )
    .unwrap();
    let state = listening::load(&store).unwrap();
    assert_eq!(state.default.bass_db, 0.5);
    assert_eq!(state.effective("Headphones").bass_db, 1.0);
}

#[test]
fn an_empty_set_command_does_not_reenable_processing_or_consume_undo() {
    let (_dir, store) = store();
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.enabled = false;
        p.reference = true;
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(store.directory.join("listening.json")).unwrap();
    assert!(sound_cli::run(&store, Some("Headphones".into()), Some(1), set_bass(None)).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("listening.json")).unwrap(),
        before
    );
}

#[test]
fn save_device_with_explicit_default_never_silently_targets_the_current_output() {
    let (_dir, store) = store();
    store
        .write_json(
            "runtime.json",
            &json!({"active":true,"updated_at_ms":analysis::now_ms(),
        "session_id":"fixture","profile_key":"Headphones","output":"Headphones"}),
        )
        .unwrap();
    assert!(sound_cli::run(&store, Some("default".into()), Some(0), Action::SaveDevice).is_err());
    assert!(!store.directory.join("listening.json").exists());
}
