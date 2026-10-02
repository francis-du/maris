//! Exercise the exact Apps-page actions with isolated state and no audio streams.
use super::*;
use crate::{
    mixer::{self, MixerState, MixerStrip},
    ui::tui::studio::controls,
};
use crossterm::event::KeyCode;
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Store, Value) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let state = mixer::edit(&store, Some(0), |config| {
        for (id, pid, sends) in [("music", 123, [1.0, 0.0]), ("voice", 456, [0.0, 1.0])] {
            config.strips.push(MixerStrip {
                id: id.into(),
                name: id.into(),
                source_device: Some(format!("pid:{pid}")),
                sends,
                ..MixerStrip::default()
            });
        }
        config.buses[0].output_device = Some("uid:speakers".into());
        config.buses[1].output_device = Some("uid:headphones".into());
        Ok(())
    })
    .unwrap();
    let runtime = json!({
        "active":true, "music_processing":true, "session_id":"mixer-ui-test", "pid":12345,
        "output":"Speakers", "profile_key":"Speakers", "sample_rate":48000,
        "device_identity":{"stable_id":"speakers"}, "device_binding_revision":0,
        "system_backend":"multi_device_mixer", "output_mode":"mixer_assignments", "rebind_count":0,
        "updated_at_ms":crate::analysis::now_ms(), "applied_revision":0,
        "requested_music_revision":0, "applied_music_revision":0,
        "mixer":{"revision":state.revision, "processing_revision":state.revision, "restart_required":false},
        "mixer_control":state
    });
    store.write_json("runtime.json", &runtime).unwrap();
    (directory, store, runtime)
}

fn key(
    store: &Store,
    runtime: &Value,
    row: usize,
    key: KeyCode,
) -> anyhow::Result<Option<crate::i18n::Notice>> {
    let mut selected = row;
    application_key(
        store,
        runtime,
        &json!({"applications":[]}),
        &mut selected,
        &mut vec![],
        None,
        key,
    )
}

#[test]
fn channel_controls_change_only_the_selected_strip_and_never_start_audio() {
    for (input, field, expected) in [
        (KeyCode::Right, "gain_db", json!(0.5)),
        (KeyCode::Left, "gain_db", json!(-0.5)),
        (KeyCode::Char(' '), "mute", json!(true)),
        (KeyCode::Char('x'), "solo", json!(true)),
        (KeyCode::Char('['), "pan", json!(-0.05)),
        (KeyCode::Char(']'), "pan", json!(0.05)),
    ] {
        let (_directory, store, runtime) = fixture();
        let before = mixer::load(&store).unwrap();
        assert!(key(&store, &runtime, 1, input).unwrap().is_some());
        let after = mixer::load(&store).unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(after.config.strips[0], before.config.strips[0]);
        assert_eq!(
            serde_json::to_value(&after.config.strips[1]).unwrap()[field],
            expected
        );
        assert_eq!(after.config.strips[1].sends, before.config.strips[1].sends);
        assert_eq!(after.config.buses, before.config.buses);
        assert_eq!(after.previous, Some(before.config));
        assert!(!store.directory.join("control.json").exists());
        assert!(!store.directory.join("profile.json").exists());
        assert!(!store.directory.join("listening.json").exists());
    }
}

#[test]
fn stale_revisions_sessions_and_pending_assignments_cannot_change_a_channel() {
    for field in [
        "session_id",
        "rebind_count",
        "updated_at_ms",
        "mixer",
        "system_backend",
    ] {
        let (_directory, store, displayed) = fixture();
        let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
        let mut current = displayed.clone();
        current[field] = match field {
            "rebind_count" => json!(1),
            "updated_at_ms" => json!(0),
            "mixer" => json!({"restart_required":true}),
            _ => json!("changed"),
        };
        store.write_json("runtime.json", &current).unwrap();
        assert!(
            key(&store, &displayed, 0, KeyCode::Right).is_err(),
            "{field}"
        );
        assert_eq!(
            std::fs::read(store.directory.join("mixer.json")).unwrap(),
            before
        );
    }
    let (_directory, store, displayed) = fixture();
    mixer::save_scene_checked(&store, Some(1), "Saved elsewhere").unwrap();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    assert!(key(&store, &displayed, 0, KeyCode::Right).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        before
    );
}

#[test]
fn capped_gain_is_a_no_op_and_undo_uses_only_mixer_history() {
    let (_directory, store, mut runtime) = fixture();
    let capped = mixer::edit(&store, Some(1), |config| {
        config.strips[0].gain_db = -60.0;
        Ok(())
    })
    .unwrap();
    runtime["mixer_control"] = serde_json::to_value(&capped).unwrap();
    let bytes = std::fs::read(store.directory.join("mixer.json")).unwrap();
    key(&store, &runtime, 0, KeyCode::Left).unwrap();
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        bytes
    );
    key(&store, &runtime, 0, KeyCode::Char('u')).unwrap();
    assert_eq!(mixer::load(&store).unwrap().config.strips[0].gain_db, 0.0);
    assert!(!store.directory.join("listening.json").exists());
}

#[test]
fn browsing_and_enter_never_write_and_a_removed_selection_does_not_retarget() {
    let (_directory, store, runtime) = fixture();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    for input in [KeyCode::Up, KeyCode::Down, KeyCode::Enter] {
        key(&store, &runtime, 0, input).unwrap();
    }
    assert!(key(&store, &runtime, usize::MAX, KeyCode::Right).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        before
    );
    let state: MixerState = serde_json::from_value(runtime["mixer_control"].clone()).unwrap();
    let mut after = state.clone();
    after.config.strips.reverse();
    assert_eq!(preserve_mixer_row(Some(&state), Some(&after), 0), 1);
    after.config.strips.retain(|strip| strip.id != "music");
    assert_eq!(
        preserve_mixer_row(Some(&state), Some(&after), 0),
        after.config.strips.len()
    );
}

#[test]
fn header_waits_for_the_actual_mixer_revision() {
    let (_directory, _store, mut runtime) = fixture();
    let now = crate::analysis::now_ms();
    assert_eq!(controls::application_state(&runtime, 0, now), "Applied");
    runtime["mixer_control"]["revision"] = json!(2);
    assert_eq!(
        controls::application_state(&runtime, 0, now),
        "Pending audio update"
    );
    runtime["mixer"]["processing_revision"] = Value::Null;
    assert_eq!(
        controls::application_state(&runtime, 0, now),
        "Apply state unknown"
    );
}
