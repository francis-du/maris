//! Exercise the exact Apps-page actions with isolated state and no audio streams.
use super::*;
use crate::{
    mixer::{self, MixerState, MixerStrip},
    ui::tui::studio::controls,
};
use crossterm::event::KeyCode;
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Store, Value) {
    fixture_in(tempfile::tempdir().unwrap())
}

fn fixture_in(directory: tempfile::TempDir) -> (tempfile::TempDir, Store, Value) {
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
    application_key_at(
        store,
        runtime,
        &json!({"applications":[]}),
        &mut selected,
        &mut vec![],
        None,
        ApplicationInput {
            key,
            now: crate::analysis::now_ms(),
        },
    )
}

// Valid-action cases model the live heartbeat without changing the pinned
// session, device or revision. Rejection cases deliberately use the raw path.
fn renew_telemetry(store: &Store, displayed: &mut Value) {
    let mut current: Value =
        crate::store::read_json(&store.directory.join("runtime.json")).unwrap();
    let now = json!(crate::analysis::now_ms());
    current["updated_at_ms"] = now.clone();
    displayed["updated_at_ms"] = now;
    store.write_json("runtime.json", &current).unwrap();
}

fn fresh_key(store: &Store, runtime: &mut Value, row: usize, input: KeyCode) {
    let _clock = crate::analysis::test_clock::Clock::freeze();
    renew_telemetry(store, runtime);
    key(store, runtime, row, input).unwrap();
}

#[test]
fn channel_controls_change_only_the_selected_strip_and_never_start_audio() {
    let _clock = crate::analysis::test_clock::Clock::freeze();
    for (input, field, expected) in [
        (KeyCode::Right, "gain_db", json!(0.5)),
        (KeyCode::Left, "gain_db", json!(-0.5)),
        (KeyCode::Char(' '), "mute", json!(true)),
        (KeyCode::Char('x'), "solo", json!(true)),
        (KeyCode::Char('['), "pan", json!(-0.05)),
        (KeyCode::Char(']'), "pan", json!(0.05)),
    ] {
        let (_directory, store, mut runtime) = fixture();
        let before = mixer::load(&store).unwrap();
        renew_telemetry(&store, &mut runtime);
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
    fresh_key(&store, &mut runtime, 0, KeyCode::Left);
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        bytes
    );
    fresh_key(&store, &mut runtime, 0, KeyCode::Char('u'));
    assert_eq!(mixer::load(&store).unwrap().config.strips[0].gain_db, 0.0);
    assert!(!store.directory.join("listening.json").exists());
}

#[test]
fn browsing_and_enter_never_write_and_a_removed_selection_does_not_retarget() {
    let (_directory, store, mut runtime) = fixture();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    for input in [KeyCode::Up, KeyCode::Down, KeyCode::Enter] {
        fresh_key(&store, &mut runtime, 0, input);
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
        usize::MAX
    );
}

#[test]
fn output_bus_gain_and_mute_edit_only_the_selected_bus() {
    for bus in 0..2 {
        for (input, field, expected) in [
            (KeyCode::Right, "gain_db", json!(0.5)),
            (KeyCode::Left, "gain_db", json!(-0.5)),
            (KeyCode::Char(' '), "mute", json!(true)),
        ] {
            let (_directory, store, mut runtime) = fixture();
            let before = mixer::load(&store).unwrap();
            fresh_key(
                &store,
                &mut runtime,
                before.config.strips.len() + bus,
                input,
            );
            let after = mixer::load(&store).unwrap();
            assert_eq!(after.revision, before.revision + 1);
            assert_eq!(after.config.strips, before.config.strips);
            assert_eq!(after.config.buses[1 - bus], before.config.buses[1 - bus]);
            assert_eq!(
                serde_json::to_value(&after.config.buses[bus]).unwrap()[field],
                expected
            );
            assert_eq!(
                after.config.buses[bus].output_device,
                before.config.buses[bus].output_device
            );
            assert_eq!(
                after.config.buses[bus].delay_ms,
                before.config.buses[bus].delay_ms
            );
            assert_eq!(after.previous, Some(before.config));
            for file in ["control.json", "profile.json", "listening.json"] {
                assert!(!store.directory.join(file).exists());
            }
        }
    }
}

#[test]
fn mixer_navigation_reaches_output_buses_without_writing() {
    let (_directory, store, runtime) = fixture();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    let mut selected = 1;
    for (key, expected) in [
        (KeyCode::Down, 2),
        (KeyCode::Down, 3),
        (KeyCode::Down, 3),
        (KeyCode::Home, 0),
        (KeyCode::End, 3),
        (KeyCode::Up, 2),
    ] {
        assert!(application_key_at(
            &store,
            &runtime,
            &json!({}),
            &mut selected,
            &mut vec![],
            None,
            ApplicationInput {
                key,
                now: crate::analysis::now_ms()
            }
        )
        .unwrap()
        .is_some());
        assert_eq!(selected, expected);
    }
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        before
    );
}

#[test]
fn output_bus_selection_survives_input_changes_by_bus_identity() {
    let (_directory, store, _) = fixture();
    let before = mixer::load(&store).unwrap();
    let mut after = before.clone();
    after.config.strips.remove(0);
    assert_eq!(preserve_mixer_row(Some(&before), Some(&after), 3), 2);
    after.config.buses.swap(0, 1);
    assert_eq!(preserve_mixer_row(Some(&before), Some(&after), 3), 1);
}

#[test]
fn removed_channel_selection_cannot_become_an_output_bus() {
    let (_directory, store, _) = fixture();
    let before = mixer::load(&store).unwrap();
    let mut after = before.clone();
    after.config.strips.remove(0);
    assert_eq!(
        preserve_mixer_row(Some(&before), Some(&after), 0),
        usize::MAX
    );
}

#[test]
fn bus_limits_no_ops_and_undo_preserve_other_controls() {
    for (gain, key_at_limit) in [(-60.0, KeyCode::Left), (6.0, KeyCode::Right)] {
        let (_directory, store, mut runtime) = fixture();
        let state = mixer::edit(&store, Some(1), |config| {
            config.buses[1].gain_db = gain;
            Ok(())
        })
        .unwrap();
        runtime["mixer_control"] = serde_json::to_value(&state).unwrap();
        let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
        for input in [
            key_at_limit,
            KeyCode::Char('x'),
            KeyCode::Char('['),
            KeyCode::Char(']'),
        ] {
            fresh_key(&store, &mut runtime, 3, input);
            assert_eq!(
                std::fs::read(store.directory.join("mixer.json")).unwrap(),
                before
            );
        }
        fresh_key(&store, &mut runtime, 3, KeyCode::Char('u'));
        let after = mixer::load(&store).unwrap();
        assert_eq!(after.config.buses[1].gain_db, 0.0);
        assert_eq!(after.config.strips, state.config.strips);
        assert_eq!(after.scenes, state.scenes);
    }
}

#[test]
fn bus_edits_reject_stale_sessions_pending_routes_and_revision_conflicts() {
    for field in ["session_id", "updated_at_ms", "system_backend", "mixer"] {
        let (_directory, store, displayed) = fixture();
        let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
        let mut current = displayed.clone();
        current[field] = match field {
            "updated_at_ms" => json!(0),
            "mixer" => json!({"restart_required":true}),
            _ => json!("changed"),
        };
        store.write_json("runtime.json", &current).unwrap();
        assert!(
            key(&store, &displayed, 3, KeyCode::Right).is_err(),
            "{field}"
        );
        assert_eq!(
            std::fs::read(store.directory.join("mixer.json")).unwrap(),
            before
        );
    }
    let (_directory, store, displayed) = fixture();
    mixer::edit(&store, Some(1), |config| {
        config.buses[1].output_device = Some("uid:other".into());
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    assert!(key(&store, &displayed, 3, KeyCode::Char(' ')).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        before
    );
}

#[test]
fn bus_ui_changes_reach_the_real_matrix_without_altering_the_other_bus() {
    for input in [KeyCode::Left, KeyCode::Right, KeyCode::Char(' ')] {
        let (_directory, store, mut runtime) = fixture();
        let before = mixer::load(&store).unwrap();
        let pcm = [[0.1, -0.1], [0.2, -0.2]];
        let mut engine = mixer::Processor::new(before.config, 48_000).unwrap();
        let reference = engine.process(&pcm);
        fresh_key(&store, &mut runtime, 3, input);
        let after = mixer::load(&store).unwrap();
        engine.update(mixer::engine::Settings::compile(&after.config, 48_000).unwrap());
        let mut rendered = [[0.0; 2]; 2];
        for _ in 0..2400 {
            rendered = engine.process(&pcm);
        }
        assert_eq!(rendered[0], reference[0]);
        let factor = match input {
            KeyCode::Left => 10.0_f32.powf(-0.5 / 20.0),
            KeyCode::Right => 10.0_f32.powf(0.5 / 20.0),
            _ => 0.0,
        };
        for channel in 0..2 {
            assert!((rendered[1][channel] - reference[1][channel] * factor).abs() < 1e-6);
        }
    }
}

#[test]
fn strip_and_bus_ids_cannot_cross_selection_namespaces() {
    let (_directory, store, _) = fixture();
    let mut before = mixer::load(&store).unwrap();
    before.config.strips[0].id = before.config.buses[0].id.clone();
    let mut after = before.clone();
    after.config.strips.remove(0);
    assert_eq!(
        preserve_mixer_row(Some(&before), Some(&after), 0),
        usize::MAX
    );
    assert_eq!(preserve_mixer_row(Some(&before), Some(&after), 2), 1);
    after.config.buses[0].id = "replacement".into();
    assert_eq!(
        preserve_mixer_row(Some(&before), Some(&after), 2),
        usize::MAX
    );
}

#[test]
fn header_waits_for_the_actual_mixer_revision() {
    let (_directory, _store, mut runtime) = fixture();
    let now = crate::analysis::now_ms();
    runtime["updated_at_ms"] = json!(now);
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

#[test]
fn a_real_expired_heartbeat_rejects_a_channel_until_the_same_target_is_live_again() {
    let clock = crate::analysis::test_clock::Clock::freeze();
    let (_directory, store, mut displayed) = fixture();
    let before = std::fs::read(store.directory.join("mixer.json")).unwrap();
    clock.advance(4_000);
    assert!(key(&store, &displayed, 1, KeyCode::Right).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        before
    );
    let pinned = displayed.clone();
    fresh_key(&store, &mut displayed, 1, KeyCode::Right);
    let mut refreshed = displayed.clone();
    refreshed["updated_at_ms"] = pinned["updated_at_ms"].clone();
    assert_eq!(
        refreshed, pinned,
        "heartbeat must preserve every target field"
    );
    assert_eq!(mixer::load(&store).unwrap().config.strips[1].gain_db, 0.5);
    assert!(!store.directory.join("control.json").exists());
    assert!(!store.directory.join("listening.json").exists());
}

#[test]
fn mixer_actions_read_before_each_clock_and_resample_inside_the_transaction() {
    use fs2::FileExt;
    use std::fs::OpenOptions;

    for row in [0, 3] {
        for input in [KeyCode::Right, KeyCode::Char(' '), KeyCode::Char('u')] {
            let (_directory, store, mut displayed) = fixture();
            displayed["updated_at_ms"] = json!(10_000);
            store.write_json("runtime.json", &displayed).unwrap();
            let pinned = displayed.clone();
            let before = mixer::load(&store).unwrap();
            let probe = OpenOptions::new()
                .read(true)
                .write(true)
                .open(store.directory.join("mixer.lock"))
                .unwrap();
            let mut samples = 0;
            let mut selected = row;
            let notice = application_key_with_clock(
                &store,
                &displayed,
                &json!({}),
                &mut selected,
                &mut vec![],
                None,
                (input, || {
                    samples += 1;
                    if samples == 1 {
                        FileExt::try_lock_exclusive(&probe).unwrap();
                        FileExt::unlock(&probe).unwrap();
                    } else {
                        assert_eq!(samples, 2, "each guard must sample exactly once");
                        let error = FileExt::try_lock_exclusive(&probe)
                            .expect_err("the second observation must hold mixer.lock");
                        assert_eq!(
                            error.raw_os_error(),
                            fs2::lock_contended_error().raw_os_error()
                        );
                    }
                    let now = 10_000 + samples - 1;
                    let mut next = displayed.clone();
                    next["updated_at_ms"] = json!(now + 1);
                    store.write_json("runtime.json", &next).unwrap();
                    now
                }),
            )
            .unwrap();
            assert!(notice.is_some());
            assert_eq!(samples, 2);
            assert_eq!(displayed, pinned, "the displayed target must stay pinned");
            let after = mixer::load(&store).unwrap();
            let mut expected = before.config.clone();
            if input == KeyCode::Char('u') {
                expected = before.previous.clone().unwrap();
            } else if row == 0 {
                if input == KeyCode::Right {
                    expected.strips[0].gain_db += 0.5;
                } else {
                    expected.strips[0].mute = true;
                }
            } else if input == KeyCode::Right {
                expected.buses[1].gain_db += 0.5;
            } else {
                expected.buses[1].mute = true;
            }
            assert_eq!(after.config, expected);
            assert_eq!(after.revision, before.revision + 1);
            assert_eq!(after.previous, Some(before.config));
            assert_eq!(after.scenes, before.scenes);
            for name in ["control.json", "profile.json", "listening.json"] {
                assert!(!store.directory.join(name).exists());
            }
        }
    }
}

#[test]
fn mixer_actions_reject_heartbeats_that_expire_or_move_into_the_future_before_lock_admission() {
    use fs2::FileExt;
    use std::fs::{self, OpenOptions};

    for future in [false, true] {
        for row in [0, 3] {
            for input in [KeyCode::Right, KeyCode::Char(' '), KeyCode::Char('u')] {
                let (_directory, store, mut displayed) = fixture();
                displayed["updated_at_ms"] = json!(10_000);
                store.write_json("runtime.json", &displayed).unwrap();
                let saved = fs::read(store.directory.join("mixer.json")).unwrap();
                let probe = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(store.directory.join("mixer.lock"))
                    .unwrap();
                let mut samples = 0;
                let mut selected = row;
                let result = application_key_with_clock(
                    &store,
                    &displayed,
                    &json!({}),
                    &mut selected,
                    &mut vec![],
                    None,
                    (input, || {
                        samples += 1;
                        if samples == 1 {
                            if future {
                                let mut next = displayed.clone();
                                next["updated_at_ms"] = json!(10_001);
                                store.write_json("runtime.json", &next).unwrap();
                            }
                            10_000
                        } else {
                            assert_eq!(samples, 2);
                            let error = FileExt::try_lock_exclusive(&probe)
                                .expect_err("freshness must be checked under mixer.lock");
                            assert_eq!(
                                error.raw_os_error(),
                                fs2::lock_contended_error().raw_os_error()
                            );
                            if future { 10_000 } else { 11_000 }
                        }
                    }),
                );
                assert!(result.is_err(), "future={future}, row={row}, key={input:?}");
                assert_eq!(samples, 2);
                assert_eq!(fs::read(store.directory.join("mixer.json")).unwrap(), saved);
                assert_eq!(displayed["updated_at_ms"], 10_000);
            }
        }
    }
}

#[test]
fn mixer_guard_uses_one_snapshot_for_identity_mode_and_restart() {
    for field in ["session_id", "system_backend", "restart_required"] {
        let (_directory, store, mut displayed) = fixture();
        displayed["updated_at_ms"] = json!(10_000);
        store.write_json("runtime.json", &displayed).unwrap();
        let saved = std::fs::read(store.directory.join("mixer.json")).unwrap();
        ensure_mixer_current(&store, &displayed, || {
            let mut next = displayed.clone();
            if field == "restart_required" {
                next["mixer"][field] = json!(true);
            } else {
                next[field] = json!("replacement");
            }
            store.write_json("runtime.json", &next).unwrap();
            10_000
        })
        .unwrap();
        assert!(ensure_mixer_current(&store, &displayed, || 10_000).is_err());
        assert_eq!(
            std::fs::read(store.directory.join("mixer.json")).unwrap(),
            saved
        );
    }
}

#[cfg(unix)]
fn run_runtime_lock_child(change: &str) {
    use std::{
        io::Read,
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    const FLAG: &str = "MARIS_MIXER_LOCK_CHILD";
    assert!(std::env::var_os(FLAG).is_none(), "child must not recurse");
    // Parent ownership also removes runtime fixtures when a stalled child is killed.
    let cleanup = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ui::tui::actions::mixer_tests::runtime_lock_handshake_child",
            "--ignored",
            "--nocapture",
        ])
        .env(FLAG, change)
        .env("MARIS_MIXER_LOCK_ROOT", cleanup.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (done, completed) = mpsc::channel();
    let output = std::thread::spawn(move || {
        let mut text = String::new();
        let result = stdout.read_to_string(&mut text).map(|_| text);
        let _ = done.send(result);
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).map(|_| text)
    });
    // This is only a failure deadline. Channel handshakes, never elapsed time,
    // order the real runtime reads and lock admission inside the owned child.
    let received = completed.recv_timeout(Duration::from_secs(30));
    if received.is_err() && child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    let status = child.wait().unwrap();
    output.join().unwrap();
    let stderr = errors.join().unwrap().unwrap();
    cleanup.close().expect("remove the owned runtime fixture root");
    let stdout = received
        .expect("owned runtime test child exceeded its cleanup deadline")
        .unwrap();
    assert!(status.success(), "{change}: {status}\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("MIXER_RUNTIME_LOCK_HANDSHAKE_COMPLETE"),
        "the exact child test did not execute: {stdout}\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn undo_rejects_session_change_before_mixer_lock_admission() {
    run_runtime_lock_child("session_id");
}

#[cfg(unix)]
#[test]
fn undo_rejects_rebind_before_mixer_lock_admission() {
    run_runtime_lock_child("rebind_count");
}

#[cfg(unix)]
#[test]
fn undo_rejects_pending_restart_before_mixer_lock_admission() {
    run_runtime_lock_child("restart_required");
}

#[cfg(unix)]
#[test]
fn unchanged_runtime_allows_both_actions_after_mixer_lock_admission() {
    run_runtime_lock_child("unchanged");
}

#[cfg(unix)]
#[test]
#[ignore = "run only by the bounded, exact-name runtime-lock parent tests"]
fn runtime_lock_handshake_child() {
    use fs2::FileExt;
    use std::{
        fs::{self, OpenOptions},
        sync::mpsc,
    };
    let change = std::env::var("MARIS_MIXER_LOCK_CHILD")
        .expect("only the owned parent test may launch this child");
    assert!(matches!(
        change.as_str(),
        "session_id" | "rebind_count" | "restart_required" | "unchanged"
    ));
    let cleanup_root = std::path::PathBuf::from(
        std::env::var_os("MARIS_MIXER_LOCK_ROOT")
            .expect("parent must own the runtime fixture root"),
    );
    for input in [KeyCode::Right, KeyCode::Char('u')] {
        let directory = tempfile::tempdir_in(&cleanup_root).unwrap();
        let (_directory, store, displayed) = fixture_in(directory);
        let before = mixer::load(&store).unwrap();
        let saved = fs::read(store.directory.join("mixer.json")).unwrap();
        let now = displayed["updated_at_ms"].as_u64().unwrap();
        let mut current = displayed.clone();
        match change.as_str() {
            "session_id" => current["session_id"] = json!("replacement-session"),
            "rebind_count" => current["rebind_count"] = json!(1),
            "restart_required" => current["mixer"]["restart_required"] = json!(true),
            _ => {}
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(store.directory.join("mixer.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();
        let (read_ready, first_read) = mpsc::channel();
        let (resume, published) = mpsc::channel();
        let worker_store = store.clone();
        let worker = std::thread::spawn(move || {
            let probe = OpenOptions::new()
                .read(true)
                .write(true)
                .open(worker_store.directory.join("mixer.lock"))
                .unwrap();
            let mut samples = 0;
            let result = application_key_with_clock(
                &worker_store,
                &displayed,
                &json!({}),
                &mut 0,
                &mut vec![],
                None,
                (input, || {
                    samples += 1;
                    match samples {
                        1 => {
                            // The initial runtime read is complete. The parent
                            // changes the file before permitting lock admission.
                            read_ready.send(()).unwrap();
                            published.recv().unwrap();
                        }
                        2 => {
                            let contention = FileExt::try_lock_exclusive(&probe)
                                .expect_err("runtime recheck must hold mixer.lock");
                            assert_eq!(
                                contention.raw_os_error(),
                                fs2::lock_contended_error().raw_os_error()
                            );
                        }
                        _ => panic!("unexpected runtime clock sample"),
                    }
                    now
                }),
            )
            .map(|notice| notice.is_some())
            .map_err(|error| format!("{error:#}"));
            assert_eq!(samples, 2, "both admission checks must observe runtime");
            result
        });
        // The channel proves the first snapshot has been read. Publishing and
        // releasing the lock before resuming cannot change that snapshot, but
        // the transaction must reject a changed session or route on its reread.
        first_read.recv().unwrap();
        store.write_json("runtime.json", &current).unwrap();
        drop(lock);
        resume.send(()).unwrap();
        let result = worker.join().unwrap();
        if change == "unchanged" {
            assert_eq!(result, Ok(true), "{input:?}");
            let after = mixer::load(&store).unwrap();
            assert_eq!(after.revision, before.revision + 1);
            assert_eq!(after.previous, Some(before.config.clone()));
            assert_eq!(after.scenes, before.scenes);
            if input == KeyCode::Char('u') {
                assert_eq!(after.config, before.previous.unwrap());
            } else {
                let mut expected = before.config;
                expected.strips[0].gain_db += 0.5;
                assert_eq!(after.config, expected);
            }
        } else {
            assert!(
                result.is_err(),
                "{input:?} accepted {change} after the pre-lock runtime snapshot"
            );
            assert_eq!(fs::read(store.directory.join("mixer.json")).unwrap(), saved);
        }
        for name in ["control.json", "profile.json", "listening.json"] {
            assert!(!store.directory.join(name).exists());
        }
    }
    println!("MIXER_RUNTIME_LOCK_HANDSHAKE_COMPLETE");
}

#[test]
fn cli_undo_keeps_its_history_and_revision_contract_without_live_telemetry() {
    let (_directory, store, _) = fixture();
    let before = mixer::load(&store).unwrap();
    store
        .write_json("runtime.json", &json!({"active":false}))
        .unwrap();
    let after = mixer::undo(&store, Some(before.revision)).unwrap();
    assert_eq!(after.config, before.previous.unwrap());
    assert_eq!(after.previous, Some(before.config));
    assert_eq!(after.revision, before.revision + 1);
    let saved = std::fs::read(store.directory.join("mixer.json")).unwrap();
    assert!(mixer::undo(&store, Some(before.revision)).is_err());
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        saved
    );

    let mut equal = MixerState::default();
    equal.previous = Some(equal.config.clone());
    store.write_json("mixer.json", &equal).unwrap();
    let after = mixer::undo(&store, Some(0)).unwrap();
    assert_eq!(after.revision, 1, "equal history still advances Undo");
    assert_eq!(after.config, equal.config);
    assert_eq!(after.previous, equal.previous);

    let empty = MixerState::default();
    store.write_json("mixer.json", &empty).unwrap();
    let saved = std::fs::read(store.directory.join("mixer.json")).unwrap();
    let error = mixer::undo(&store, Some(0)).unwrap_err();
    assert!(error.to_string().contains("Nothing to restore in mixer"));
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        saved
    );
}

#[test]
fn undo_admission_check_holds_the_mixer_lock_and_releases_it_on_rejection() {
    use fs2::FileExt;
    use std::fs::{self, OpenOptions};

    let (_directory, store, _) = fixture();
    let before = mixer::load(&store).unwrap();
    let saved = fs::read(store.directory.join("mixer.json")).unwrap();
    let probe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.directory.join("mixer.lock"))
        .unwrap();
    let mut checked = false;
    let error = mixer::undo_checked(&store, Some(before.revision), || {
        checked = true;
        let contention = FileExt::try_lock_exclusive(&probe)
            .expect_err("Undo admission check ran without holding mixer.lock");
        assert_eq!(
            contention.raw_os_error(),
            fs2::lock_contended_error().raw_os_error(),
            "the second handle must fail because the mixer lock is held"
        );
        anyhow::bail!("test Undo admission rejected")
    })
    .unwrap_err();
    assert!(checked, "Undo must execute its admission check");
    assert_eq!(error.to_string(), "test Undo admission rejected");
    assert_eq!(fs::read(store.directory.join("mixer.json")).unwrap(), saved);

    FileExt::try_lock_exclusive(&probe).expect("rejected Undo must release mixer.lock");
    FileExt::unlock(&probe).unwrap();
}
