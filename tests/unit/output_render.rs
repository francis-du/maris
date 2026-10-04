use super::*;
use crate::{music::MusicProfile, profile::Profile};

#[test]
fn a_full_queue_does_not_acknowledge_a_device_limit_change() {
    use crate::{control::store::Store, devices::capability};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let updates = ArrayQueue::new(8);
    let mut eq = 0;
    let mut music = Some(0);
    let original = capability::effective(&store, "Headphones").unwrap();
    let mut sent = original.clone();
    let restricted = capability::Capability {
        max_preference_boost_db: 0.0,
        ..original.clone()
    };
    for _ in 0..8 {
        assert!(updates
            .push(Update {
                settings: settings(),
                revision: 0,
                music_revision: 0
            })
            .is_ok());
    }
    store
        .write_json(
            "device-capabilities.json",
            &serde_json::json!({"revision":1,"devices":{"Headphones":restricted}}),
        )
        .unwrap();
    super::super::session_control::queue_settings(
        &store,
        "Headphones",
        48_000,
        &updates,
        &mut eq,
        &mut music,
        &mut sent,
    )
    .unwrap();
    assert_eq!(
        sent, original,
        "full queue acknowledged a limit that was not delivered"
    );
    while updates.pop().is_some() {}
    super::super::session_control::queue_settings(
        &store,
        "Headphones",
        48_000,
        &updates,
        &mut eq,
        &mut music,
        &mut sent,
    )
    .unwrap();
    assert_eq!(sent, restricted);
    assert_eq!(updates.len(), 1);
    updates.pop();
    super::super::session_control::queue_settings(
        &store,
        "Headphones",
        48_000,
        &updates,
        &mut eq,
        &mut music,
        &mut sent,
    )
    .unwrap();
    assert!(
        updates.is_empty(),
        "unchanged limits caused redundant updates"
    );
}

#[test]
fn device_limit_changes_reach_audio_without_a_preference_revision_change() {
    use crate::{control::store::Store, devices::capability, tuning::preferences};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    const KEY: &str = "Headphones";
    preferences::edit(&store, Some(0), Some(KEY), |profile| {
        profile.bass_db = 3.0;
        Ok(())
    })
    .unwrap();
    let updates = ArrayQueue::new(8);
    let mut eq = 0;
    let mut music = Some(0);
    let mut sent_capability = capability::effective(&store, KEY).unwrap();
    super::super::session_control::queue_settings(
        &store,
        KEY,
        48_000,
        &updates,
        &mut eq,
        &mut music,
        &mut sent_capability,
    )
    .unwrap();
    let before = updates.pop().unwrap();
    let profile_bytes = std::fs::read(store.directory.join("listening.json")).unwrap();
    let restricted = capability::Capability {
        max_preference_boost_db: 0.0,
        ..capability::effective(&store, KEY).unwrap()
    };
    store
        .write_json(
            "device-capabilities.json",
            &serde_json::json!({"revision":1,"devices":{"Headphones":restricted}}),
        )
        .unwrap();
    assert_eq!(capability::effective(&store, KEY).unwrap(), restricted);
    super::super::session_control::queue_settings(
        &store,
        KEY,
        48_000,
        &updates,
        &mut eq,
        &mut music,
        &mut sent_capability,
    )
    .unwrap();
    let changed = updates
        .pop()
        .expect("device limit changed but no audio update was sent");
    assert!(before.settings != changed.settings);
    assert_eq!(changed.revision, before.revision);
    assert_eq!(changed.music_revision, before.music_revision);
    let expected = Settings::compile(&store.load().unwrap().profile, 48_000)
        .unwrap()
        .with_music(
            &capability::apply_constraints(
                preferences::load(&store).unwrap().effective(KEY),
                &restricted,
            ),
            48_000,
        )
        .unwrap();
    assert!(changed.settings == expected);
    assert_eq!(
        std::fs::read(store.directory.join("listening.json")).unwrap(),
        profile_bytes
    );
}

#[test]
fn output_handoff_preserves_old_stream_on_sync_and_async_startup_errors() {
    use std::cell::RefCell;
    use std::rc::Rc;
    struct Stream(u8, Rc<RefCell<Vec<u8>>>);
    impl Drop for Stream {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }
    for asynchronous in [false, true] {
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut active = vec![Stream(1, dropped.clone())];
        let old = Metrics::default();
        let next = Metrics::default();
        next.output_quarantined.store(true, Ordering::Release);
        let result = handoff_output(&mut active, Stream(2, dropped.clone()), &old, &next, |_| {
            if asynchronous {
                next.errors.store(1, Ordering::Release);
                next.callback_ready.store(true, Ordering::Release);
                Ok(())
            } else {
                anyhow::bail!("injected play rejection")
            }
        });
        assert!(result.is_err());
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, 1);
        assert_eq!(*dropped.borrow(), vec![2]);
        assert!(!old.stopping.load(Ordering::Acquire));
    }
}

#[test]
fn output_handoff_retires_old_stream_before_unquarantining_replacement() {
    struct Stream(u8, Arc<Metrics>);
    impl Drop for Stream {
        fn drop(&mut self) {
            if self.0 == 1 {
                assert!(self.1.output_quarantined.load(Ordering::Acquire));
            }
        }
    }
    let old = Metrics::default();
    old.faded_out.store(true, Ordering::Release);
    let next = Arc::new(Metrics::default());
    next.output_quarantined.store(true, Ordering::Release);
    let mut active = vec![Stream(1, next.clone())];
    handoff_output(&mut active, Stream(2, next.clone()), &old, &next, |_| {
        next.callback_ready.store(true, Ordering::Release);
        Ok(())
    })
    .unwrap();
    assert_eq!(active[0].0, 2);
    assert!(!next.output_quarantined.load(Ordering::Acquire));
}

#[test]
fn a_start_request_without_a_callback_is_not_readiness() {
    let metrics = Metrics::default();
    assert!(wait_output_ready(&metrics, Duration::ZERO).is_err());
    metrics.callback_ready.store(true, Ordering::Release);
    wait_output_ready(&metrics, Duration::ZERO).unwrap();
    metrics.errors.store(1, Ordering::Release);
    assert!(wait_output_ready(&metrics, Duration::ZERO).is_err());
}

#[test]
fn replacement_output_observes_capture_gaps_without_inheriting_old_output_errors() {
    let queue = Arc::new(ArrayQueue::new(4096));
    let capture = Arc::new(Metrics::default());
    let output = Metrics::default();
    let mut source = LiveSource::new(queue.clone(), 48_000).with_capture_metrics(capture.clone());
    for _ in 0..2048 {
        queue.push([0.25; 2]).unwrap();
    }
    assert!(source.frame(&output).1);
    capture.capture_discontinuities.store(1, Ordering::Release);
    assert!(!source.frame(&output).1);
    assert!(queue.is_empty());
    assert_eq!(output.stream_resets.load(Ordering::Relaxed), 1);
    for _ in 0..2048 {
        queue.push([-0.25; 2]).unwrap();
    }
    assert!(source.frame(&output).1);
    assert_eq!(output.errors.load(Ordering::Relaxed), 0);
}

#[test]
fn prepared_replacement_proves_callback_readiness_without_consuming_or_publishing() {
    let metrics = Arc::new(Metrics::default());
    metrics.output_quarantined.store(true, Ordering::Release);
    let updates = Arc::new(ArrayQueue::new(8));
    assert!(updates
        .push(Update {
            settings: settings(),
            revision: 7,
            music_revision: 8
        })
        .is_ok());
    let mut renderer = Renderer::new(settings(), 48_000, updates.clone(), metrics.clone());
    let mut data = [1.0_f32; 480];
    renderer.render(&mut data, 2, || {
        panic!("uncommitted replacement consumed shared PCM")
    });
    assert!(data.iter().all(|x| *x == 0.0));
    assert!(metrics.callback_ready.load(Ordering::Acquire));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 0);
    assert_eq!(updates.len(), 1);
    assert!(!metrics.source_started.load(Ordering::Relaxed));
    metrics.output_quarantined.store(false, Ordering::Release);
    renderer.render(&mut data, 2, || ([0.25, -0.25], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 7);
    assert_eq!(metrics.music_revision.load(Ordering::Relaxed), 8);
    assert!(data[0].abs() < 0.001);
    assert!(data.iter().any(|x| *x != 0.0));
}

#[test]
fn native_output_error_forces_the_next_callback_to_restart_from_silence() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        settings(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let now = Instant::now();
    let mut data = [0.0_f32; 512];
    let period = Duration::from_secs_f64(256.0 / 48_000.0);
    for index in 0..12_u32 {
        renderer.render_at(
            &mut data,
            2,
            || ([0.5, -0.5], true),
            now + period.mul_f64(f64::from(index)),
        );
    }
    assert!((data[0] - 0.5).abs() < 1e-5);

    output_stream_error(&metrics);
    assert_eq!(metrics.errors.load(Ordering::Relaxed), 1);
    renderer.render_at(
        &mut data,
        2,
        || ([-0.5, 0.5], true),
        now + period.mul_f64(12.0),
    );
    assert!(
        data[0].abs() < 0.001,
        "stream error resumed old/new PCM without a silence-bound recovery"
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 1);
}

#[test]
fn a_short_missed_native_deadline_restarts_before_replaying_old_pcm() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        settings(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let now = Instant::now();
    let mut data = [0.0_f32; 512]; // 256 stereo frames ~= 5.33 ms.
    let period = Duration::from_secs_f64(256.0 / 48_000.0);
    for index in 0..12_u32 {
        renderer.render_at(
            &mut data,
            2,
            || ([0.5, -0.5], true),
            now + period.mul_f64(f64::from(index)),
        );
    }
    assert!((data[0] - 0.5).abs() < 1e-5);

    let previous = now + period.mul_f64(11.0);
    renderer.render_at(
        &mut data,
        2,
        || ([-0.5, 0.5], true),
        previous + Duration::from_millis(11),
    );
    assert!(
        data[0].abs() < 0.001,
        "a short missed hardware deadline replayed queued PCM at full amplitude"
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 1);
}

#[test]
fn native_callback_jitter_inside_the_deadline_budget_does_not_reset_audio() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        settings(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let now = Instant::now();
    let mut data = [0.0_f32; 512];
    let period = Duration::from_secs_f64(256.0 / 48_000.0);
    for index in 0..12_u32 {
        renderer.render_at(
            &mut data,
            2,
            || ([0.25, -0.25], true),
            now + period.mul_f64(f64::from(index)),
        );
    }
    let previous = now + period.mul_f64(11.0);
    renderer.render_at(
        &mut data,
        2,
        || ([0.25, -0.25], true),
        previous + Duration::from_millis(8),
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 0);
    assert!((data[0] - 0.25).abs() < 1e-5);
}

#[test]
fn a_late_native_callback_restarts_from_silence_without_replaying_old_pcm() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        settings(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let now = Instant::now();
    let mut data = [0.0_f32; 480];
    for index in 0..20 {
        renderer.render_at(
            &mut data,
            2,
            || ([0.5, -0.5], true),
            now + Duration::from_millis(index * 5),
        );
    }
    assert!((data[0] - 0.5).abs() < 1e-5);
    renderer.render_at(
        &mut data,
        2,
        || ([-0.5, 0.5], true),
        now + Duration::from_secs(1),
    );
    assert!(
        data[0].abs() < 0.001,
        "late callback returned at full amplitude"
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 1);
    renderer.render_at(
        &mut data,
        2,
        || ([-0.5, 0.5], true),
        now + Duration::from_millis(1005),
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 1);
}

#[test]
fn valid_large_callback_blocks_are_not_mistaken_for_a_power_gap() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        settings(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let now = Instant::now();
    let mut data = vec![0.0_f32; 19_200]; // 200 ms of actual stereo frames.
    renderer.render_at(&mut data, 2, || ([0.1, -0.1], true), now);
    renderer.render_at(
        &mut data,
        2,
        || ([0.1, -0.1], true),
        now + Duration::from_millis(200),
    );
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Relaxed), 0);
    assert!((data[0] - 0.1).abs() < 1e-5);
}

#[test]
fn configuration_events_quarantine_pcm_until_the_observed_generation_is_checked() {
    let queue = Arc::new(ArrayQueue::new(4096));
    let metrics = Metrics::default();
    let mut source = LiveSource::new(queue.clone(), 48_000);
    metrics.configuration_events.store(2, Ordering::Release);
    assert!(!source.frame(&metrics).1);
    for _ in 0..2048 {
        queue.push([0.25, -0.25]).unwrap();
    }
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    assert!(
        !source.frame(&metrics).1,
        "checking event 1 accepted unchecked event 2"
    );
    metrics
        .checked_configuration_events
        .store(2, Ordering::Release);
    assert!(source.frame(&metrics).1);
    assert_eq!(metrics.stream_resets.load(Ordering::Relaxed), 1);
}

#[test]
fn capture_overflow_discards_stale_backlog_and_rearms_the_reserve() {
    let queue = Arc::new(ArrayQueue::new(4096));
    let metrics = Metrics::default();
    let mut source = LiveSource::new(queue.clone(), 48_000);
    for _ in 0..4096 {
        queue.push([0.4, -0.4]).unwrap();
    }
    for _ in 0..1024 {
        source.frame(&metrics);
    }
    metrics.overruns.store(1, Ordering::Relaxed);
    let (frame, ready) = source.frame(&metrics);
    assert!(
        !ready,
        "overflowed old PCM was treated as continuous live audio"
    );
    assert_eq!(frame, [0.0; 2]);
    assert!(
        queue.is_empty(),
        "interrupted playback retained stale backlog"
    );
    for _ in 0..1024 {
        queue.push([-0.3, 0.3]).unwrap();
    }
    let mut previous = [0.0_f32; 2];
    for _ in 0..512 {
        let (frame, ready) = source.frame(&metrics);
        assert!(ready);
        assert!(
            frame[0] <= 0.001,
            "a pre-reset positive sample leaked into the new stream"
        );
        assert!((frame[0] - previous[0]).abs() < 0.003);
        previous = frame;
    }
    assert!(previous[0] < -0.25);
}

#[test]
fn interruption_and_early_recovery_do_not_step_between_unrelated_samples() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        for gap in [1, 16, rate / 200, rate / 10] {
            let settings = Settings::compile(&Profile::default(), rate).unwrap();
            let mut render = RenderState::new(settings, rate);
            let mut previous = [0.0_f32; 2];
            for _ in 0..rate / 20 {
                previous = render.frame([0.5, -0.25], true, false);
            }
            let max_step = 2.0 / (rate as f32 * 0.025);
            for _ in 0..gap {
                let frame = render.frame([0.0; 2], false, false);
                for channel in 0..2 {
                    assert!(
                        (frame[channel] - previous[channel]).abs() <= max_step,
                        "{rate} Hz / {gap} absent frames: dropout made an abrupt sample step"
                    );
                }
                previous = frame;
            }
            for _ in 0..rate / 20 {
                let frame = render.frame([-0.5, 0.25], true, false);
                for channel in 0..2 {
                    assert!(
                        (frame[channel] - previous[channel]).abs() <= max_step,
                        "{rate} Hz / {gap} absent frames: recovery made an abrupt sample step"
                    );
                    assert!(frame[channel].is_finite() && frame[channel].abs() <= 0.891252);
                }
                previous = frame;
            }
            assert!((previous[0] + 0.5).abs() < 1e-5);
        }
    }
}

#[test]
fn unready_stream_cannot_inject_garbage_after_initial_playback() {
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..2400 {
        render.frame([0.2, -0.2], true, false);
    }
    for _ in 0..2400 {
        let output = render.frame([16.0, -16.0], false, false);
        assert!(
            output.iter().all(|value| value.abs() <= 0.200001),
            "invalid-source PCM reached output"
        );
    }
    assert_eq!(render.frame([16.0, -16.0], false, false), [0.0; 2]);
}

#[test]
fn preparing_an_output_does_not_publish_settings_before_a_callback_runs() {
    let metrics = Arc::new(Metrics::default());
    metrics
        .effective_gain
        .store((-2.0_f32).to_bits(), Ordering::Relaxed);
    let profile = Profile {
        preamp_db: -12.0,
        bypass: true,
        ..Profile::default()
    };
    let next = Settings::compile(&profile, 48_000).unwrap();
    let mut renderer = Renderer::new(next, 48_000, Arc::new(ArrayQueue::new(8)), metrics.clone());
    assert!(
        !metrics.tonal_bypass.load(Ordering::Relaxed),
        "unstarted replacement changed the active status"
    );
    assert_eq!(
        f32::from_bits(metrics.effective_gain.load(Ordering::Relaxed)),
        -2.0
    );
    renderer.render(&mut [0.0_f32; 32], 2, || ([0.0; 2], false));
    assert!(metrics.tonal_bypass.load(Ordering::Relaxed));
    assert_eq!(
        f32::from_bits(metrics.effective_gain.load(Ordering::Relaxed)),
        next.effective_preamp_db() as f32
    );
}

#[test]
fn renderer_reports_level_match_makeup_separately_from_static_safety_preamp() {
    let metrics = Arc::new(Metrics::default());
    let profile = Profile::default();
    let music = crate::music::MusicProfile {
        bass_db: 6.0,
        adaptive: crate::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..crate::music::MusicProfile::default()
    };
    let settings = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap();
    assert!(settings.effective_preamp_db() < -5.0);
    let mut renderer = Renderer::new(
        settings,
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    renderer.render(&mut [0.0_f32; 256], 2, || ([0.01, 0.01], true));
    let makeup = f32::from_bits(metrics.level_match_makeup.load(Ordering::Relaxed));
    assert!(
        makeup > 5.0,
        "renderer hid active level-match makeup from telemetry: {makeup:.3} dB"
    );
    assert_eq!(
        f32::from_bits(metrics.effective_gain.load(Ordering::Relaxed)),
        settings.effective_preamp_db() as f32
    );
}

#[test]
fn all_settings_rows_commit_through_the_session_queue_into_the_real_renderer() {
    // This fixture represents a live session, independent of CI disk/scheduler delays.
    let _clock = crate::analysis::test_clock::Clock::freeze();
    use crate::{
        configuration::{Context, Editor, Outcome},
        device_profile, listening,
        store::Store,
    };
    use crossterm::event::KeyCode;
    use serde_json::json;
    const KEY: &str = "OFFLINE Renderer Speaker";
    for initial_row in 0..crate::ui::tui::input::SOUND_ROWS {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path());
        let capability = device_profile::effective(&store, KEY).unwrap();
        let runtime = json!({"active":true,"session_id":"renderer-test","output":KEY,
            "profile_key":KEY,"sample_rate":48000,"music_processing":true,
            "device_capability":capability,"updated_at_ms":crate::analysis::now_ms()});
        store.write_json("runtime.json", &runtime).unwrap();
        let eq = store.load().unwrap();
        let library = listening::load(&store).unwrap();
        let context = Context {
            runtime: &runtime,
            eq: &eq,
            listening: &library,
        };
        let baseline = Settings::compile(&eq.profile, 48_000)
            .unwrap()
            .with_music(library.effective(KEY), 48_000)
            .unwrap();
        let updates = Arc::new(ArrayQueue::new(8));
        let metrics = Arc::new(Metrics::default());
        let mut renderer = Renderer::new(baseline, 48_000, updates.clone(), metrics.clone());
        let mut sent_eq = 0;
        let mut sent_music = Some(0);
        let mut sent_capability = capability.clone();
        let mut editor = Editor::default();
        let mut row = initial_row;
        let direction = if matches!(row, 5 | 10 | 14 | 15) {
            '-'
        } else {
            '+'
        };
        assert_eq!(
            editor
                .handle(&store, context, &mut row, KeyCode::Char(direction))
                .unwrap(),
            Outcome::Staged
        );
        assert!(editor.pending(), "row {row} did not stage a real change");
        super::super::session_control::queue_settings(
            &store,
            KEY,
            48_000,
            &updates,
            &mut sent_eq,
            &mut sent_music,
            &mut sent_capability,
        )
        .unwrap();
        assert!(
            updates.is_empty(),
            "draft reached the audio queue before Enter"
        );
        assert!(matches!(
            editor
                .handle(&store, context, &mut row, KeyCode::Enter)
                .unwrap(),
            Outcome::Applied(Some(_))
        ));
        let saved_eq = store.load().unwrap();
        let saved_music = listening::load(&store).unwrap();
        assert_eq!(saved_eq.revision + saved_music.revision, 1);
        super::super::session_control::queue_settings(
            &store,
            KEY,
            48_000,
            &updates,
            &mut sent_eq,
            &mut sent_music,
            &mut sent_capability,
        )
        .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(
            metrics.revision.load(Ordering::Relaxed)
                + metrics.music_revision.load(Ordering::Relaxed),
            0,
            "enqueue must not claim callback application"
        );
        let expected = Settings::compile(&saved_eq.profile, 48_000)
            .unwrap()
            .with_music(
                &device_profile::apply_constraints(saved_music.effective(KEY), &capability),
                48_000,
            )
            .unwrap();
        let queued = updates.pop().unwrap();
        assert!(
            queued.settings == expected,
            "row {row}: saved parameters and DSP settings diverged"
        );
        assert!(updates.push(queued).is_ok());
        let mut output = vec![0.0_f32; 24_000];
        let mut index = 0;
        renderer.render(&mut output, 2, || {
            let x = (std::f32::consts::TAU * 125.0 * index as f32 / 48_000.0).sin() * 0.1;
            index += 1;
            ([x, -x * 0.5], true)
        });
        assert!(updates.is_empty());
        assert_eq!(metrics.revision.load(Ordering::Relaxed), saved_eq.revision);
        assert_eq!(
            metrics.music_revision.load(Ordering::Relaxed),
            saved_music.revision
        );
        assert!(output.iter().all(|x| x.is_finite() && x.abs() <= 0.891252));
        assert!(output.iter().any(|x| x.abs() > 0.001));
        if matches!(row, 0 | 10 | 18) {
            let mut before = RenderState::new(baseline, 48_000);
            let mut difference = 0.0_f64;
            for (i, frame) in output.as_chunks::<2>().0.iter().enumerate() {
                let x = (std::f32::consts::TAU * 125.0 * i as f32 / 48_000.0).sin() * 0.1;
                let original = before.frame([x, -x * 0.5], true, false);
                if i > 6000 {
                    difference += f64::from(frame[0] - original[0]).powi(2);
                }
            }
            assert!(
                difference > 1e-6,
                "row {row}: revisions changed but audio samples did not"
            );
        }
    }
}

#[test]
fn a_full_settings_queue_retries_and_applies_the_latest_saved_revision() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::store::Store::at(directory.path());
    let updates = Arc::new(ArrayQueue::new(8));
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(settings(), 48_000, updates.clone(), metrics.clone());
    let mut sent_eq = 0;
    let mut sent_music = Some(0);
    let mut sent_capability = crate::device_profile::effective(&store, "OFFLINE fixture").unwrap();
    for revision in 1..=9 {
        store
            .edit(Some(revision - 1), |p| {
                p.preamp_db = -(revision as f64);
                Ok(())
            })
            .unwrap();
        super::super::session_control::queue_settings(
            &store,
            "OFFLINE fixture",
            48_000,
            &updates,
            &mut sent_eq,
            &mut sent_music,
            &mut sent_capability,
        )
        .unwrap();
    }
    assert_eq!(updates.len(), 8);
    assert_eq!(
        sent_eq, 8,
        "full queue falsely acknowledged unsent revision 9"
    );
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 0);
    let mut output = [0.0_f32; 960];
    renderer.render(&mut output, 2, || ([0.1, -0.1], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 8);
    super::super::session_control::queue_settings(
        &store,
        "OFFLINE fixture",
        48_000,
        &updates,
        &mut sent_eq,
        &mut sent_music,
        &mut sent_capability,
    )
    .unwrap();
    assert_eq!(sent_eq, 9);
    renderer.render(&mut output, 2, || ([0.1, -0.1], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 9);
    assert!(updates.is_empty());
    super::super::session_control::queue_settings(
        &store,
        "OFFLINE fixture",
        48_000,
        &updates,
        &mut sent_eq,
        &mut sent_music,
        &mut sent_capability,
    )
    .unwrap();
    assert!(updates.is_empty(), "unchanged settings were resent");
}

fn settings() -> Settings {
    Settings::compile(&Profile::default(), 48_000).unwrap()
}

#[test]
fn startup_envelope_waits_for_actual_capture_frames_not_output_callbacks() {
    let metrics = Metrics::default();
    let queue = Arc::new(ArrayQueue::new(8192));
    let mut source = Source::Live(LiveSource::new(queue.clone(), 48_000));
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..48_000 {
        let (input, ready) = source.frame(&metrics);
        assert!(!ready);
        assert_eq!(render.frame(input, ready, false), [0.0; 2]);
    }
    assert!(!render.started);
    assert_eq!(render.gain, 0.0);
    for _ in 0..4096 {
        queue.push([0.5, -0.5]).unwrap();
    }
    let (input, ready) = source.frame(&metrics);
    assert!(ready);
    let first = render.frame(input, ready, false);
    assert!(render.started);
    assert!(render.gain < 0.001);
    assert!(first[0].abs() < 0.001);
    assert_eq!(first[0], -first[1]);
}

#[test]
fn queued_tonal_transition_is_identical_after_arbitrary_capture_startup_delay() {
    let profile = Profile::default();
    let music = MusicProfile::preset("detail").unwrap();
    let baseline = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music.output_switch_baseline(), 48_000)
        .unwrap();
    let target = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let (baseline, target) = baseline.share_transition_headroom(target);
    assert_eq!(baseline.effective_preamp_db(), target.effective_preamp_db());
    let mut direct = RenderState::new(baseline, 48_000);
    let mut delayed = RenderState::new(baseline, 48_000);
    direct.processor.update(target);
    delayed.processor.update(target);
    for _ in 0..24_000 {
        assert_eq!(delayed.frame([0.0; 2], false, false), [0.0; 2]);
    }
    for index in 0..12_000 {
        let x = (std::f32::consts::TAU * 8_000.0 * index as f32 / 48_000.0).sin() * 0.3;
        assert_eq!(
            direct.frame([x, -x], true, false),
            delayed.frame([x, -x], true, false)
        );
    }
}

#[test]
fn valid_digital_silence_starts_the_envelope_without_amplitude_gating() {
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..2400 {
        assert_eq!(render.frame([0.0; 2], true, false), [0.0; 2]);
    }
    assert!(render.started);
    assert_eq!(render.gain, 1.0);
}

#[test]
fn stop_fade_reaches_zero_monotonically_even_after_source_disappears() {
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..2400 {
        render.frame([0.1; 2], true, false);
    }
    let mut previous = 1.0;
    for _ in 0..2400 {
        render.frame([0.1; 2], false, true);
        assert!(render.gain <= previous);
        previous = render.gain;
    }
    assert_eq!(render.gain, 0.0);
    assert_eq!(render.frame([1.0; 2], false, true), [0.0; 2]);
}

#[test]
fn expired_renderer_telemetry_still_rejects_a_staged_configuration() {
    use crate::{
        configuration::{Context, Editor, Outcome},
        device_profile, listening,
        store::Store,
    };
    use crossterm::event::KeyCode;
    use serde_json::json;
    let clock = crate::analysis::test_clock::Clock::freeze();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let key = "OFFLINE expiry fixture";
    let runtime = json!({"active":true,"session_id":"expiry-test","output":key,
        "profile_key":key,"sample_rate":48000,"music_processing":true,
        "device_capability":device_profile::effective(&store,key).unwrap(),
        "updated_at_ms":crate::analysis::now_ms()});
    store.write_json("runtime.json", &runtime).unwrap();
    let eq = store.load().unwrap();
    let library = listening::load(&store).unwrap();
    let context = Context {
        runtime: &runtime,
        eq: &eq,
        listening: &library,
    };
    let mut editor = Editor::default();
    let mut row = 0;
    assert_eq!(
        editor
            .handle(&store, context, &mut row, KeyCode::Char('+'))
            .unwrap(),
        Outcome::Staged
    );
    clock.advance(10_000);
    assert_eq!(crate::audio::runtime_status(&store)["stale"], true);
    assert!(editor
        .handle(&store, context, &mut row, KeyCode::Enter)
        .is_err());
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!directory.path().join("control.json").exists());
}
