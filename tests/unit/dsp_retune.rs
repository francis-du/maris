use super::*;

#[test]
fn retune_resets_downstream_outer_eq_state_after_upstream_change() {
    let mut old_profile = Profile::default();
    old_profile.bands[0].gain_db = 6.0;
    old_profile.bands[1].gain_db = 6.0;
    let mut new_profile = old_profile.clone();
    new_profile.bands[0].gain_db = -6.0;

    let old = Settings::compile(&old_profile, 48_000).unwrap();
    let new = Settings::compile(&new_profile, 48_000).unwrap();
    let mut trained = Chain::new(old);
    let _ = trained.frame([0.5, 0.5]);

    let mut retuned = trained.retune(new);
    let mut fresh = Chain::new(new);
    let actual = retuned.frame([0.0, 0.0]);
    let expected = fresh.frame([0.0, 0.0]);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-12,
            "outer downstream biquad leaked stale state after upstream EQ change: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_resets_crossfeed_lowpass_state_when_width_changes_its_input() {
    let old_profile = Profile {
        crossfeed: 0.2,
        ..Profile::default()
    };
    let mut new_profile = old_profile.clone();
    new_profile.stereo_width = 1.25;

    let old = Settings::compile(&old_profile, 48_000).unwrap();
    let new = Settings::compile(&new_profile, 48_000).unwrap();
    let mut trained = Chain::new(old);
    let _ = trained.frame([0.5, -0.25]);

    let mut retuned = trained.retune(new);
    let mut fresh = Chain::new(new);
    let actual = retuned.frame([0.0, 0.0]);
    let expected = fresh.frame([0.0, 0.0]);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-12,
            "crossfeed lowpass leaked stale state after width changed its feed: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn rapid_crossfeed_reversals_stay_continuous_and_settle_on_latest_target() {
    let dry_profile = Profile::default();
    let wet_profile = Profile {
        crossfeed: 0.3,
        ..Profile::default()
    };
    let dry = Settings::compile(&dry_profile, 48_000).unwrap();
    let wet = Settings::compile(&wet_profile, 48_000).unwrap();
    let mut processor = Processor::new(dry);

    let mut previous = [0.0_f32; 2];
    let mut maximum_step = 0.0_f32;
    for i in 0..8_000 {
        if i == 2_000 {
            processor.update(wet);
        } else if i == 2_300 {
            processor.update(dry);
        } else if i == 2_450 {
            processor.update(wet);
        }
        let x = (0.1 * (std::f64::consts::TAU * 70.0 * i as f64 / 48_000.0).sin()) as f32;
        let output = processor.process([x, 0.0]);
        if i > 1 {
            maximum_step = maximum_step
                .max((output[0] - previous[0]).abs())
                .max((output[1] - previous[1]).abs());
        }
        previous = output;
    }
    assert!(
        maximum_step < 0.003,
        "rapid crossfeed reversal produced a click-sized sample step: {maximum_step}"
    );
    assert!(
        !processor.settings_pending(),
        "latest crossfeed target never settled after reversal"
    );

    let mut fresh = Processor::new(wet);
    let mut actual_power = [0.0_f64; 2];
    let mut expected_power = [0.0_f64; 2];
    for i in 0..96_000 {
        let x = (0.1 * (std::f64::consts::TAU * 70.0 * i as f64 / 48_000.0).sin()) as f32;
        let actual = processor.process([x, 0.0]);
        let expected = fresh.process([x, 0.0]);
        if i > 48_000 {
            for channel in 0..2 {
                actual_power[channel] += f64::from(actual[channel]).powi(2);
                expected_power[channel] += f64::from(expected[channel]).powi(2);
            }
        }
    }
    for channel in 0..2 {
        let delta_db = 10.0 * (actual_power[channel] / expected_power[channel]).log10();
        assert!(
            delta_db.abs() < 0.02,
            "rapid crossfeed reversals left stale state in channel {channel}: {delta_db:.4} dB"
        );
    }
}

#[test]
fn completed_single_frame_retune_is_already_audibly_at_target() {
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let target_profile = Profile {
        stereo_width: 0.0,
        ..Profile::default()
    };
    let mut target = Settings::compile(&target_profile, 48_000).unwrap();
    target.transition_frames = 1;

    let mut processor = Processor::new(base);
    processor.update(target);
    assert!(processor.settings_pending());

    let input = [0.1_f32, -0.1_f32];
    let actual = processor.process(input);
    assert!(
        !processor.settings_pending(),
        "single-frame retune remained pending after its only transition frame"
    );

    let mut fresh = Processor::new(target);
    let expected = fresh.process(input);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-7,
            "retune reported complete before the audible target was reached: actual={actual:?} expected={expected:?}"
        );
    }
}

#[test]
fn single_frame_retune_endpoint_is_sample_rate_invariant() {
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let base = Settings::compile(&Profile::default(), rate).unwrap();
        let target_profile = Profile {
            stereo_width: 0.0,
            ..Profile::default()
        };
        let mut target = Settings::compile(&target_profile, rate).unwrap();
        target.transition_frames = 1;

        let mut processor = Processor::new(base);
        processor.update(target);
        let input = [0.1_f32, -0.1_f32];
        let actual = processor.process(input);
        assert!(
            !processor.settings_pending(),
            "single-frame retune remained pending at {rate} Hz"
        );

        let mut fresh = Processor::new(target);
        let expected = fresh.process(input);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-7,
                "single-frame retune endpoint changed with sample rate {rate}: actual={actual:?} expected={expected:?}"
            );
        }
    }
}

#[test]
fn discontinuity_during_queued_retune_preserves_latest_target_and_progress() {
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let middle_profile = Profile {
        stereo_width: 0.6,
        ..Profile::default()
    };
    let final_profile = Profile {
        stereo_width: 0.0,
        ..Profile::default()
    };
    let mut middle = Settings::compile(&middle_profile, 48_000).unwrap();
    let mut final_target = Settings::compile(&final_profile, 48_000).unwrap();
    middle.transition_frames = 64;
    final_target.transition_frames = 64;

    let mut processor = Processor::new(base);
    processor.update(middle);
    for _ in 0..17 {
        let _ = processor.process([0.1, -0.1]);
    }
    processor.update(final_target);
    assert!(processor.settings_pending());
    let remaining_before_reset = processor.remaining;

    processor.reset_history();
    assert_eq!(
        processor.remaining, remaining_before_reset,
        "discontinuity restarted or shortened the in-flight retune"
    );
    assert!(
        processor.pending.is_some(),
        "discontinuity dropped the queued latest target"
    );

    for _ in 0..160 {
        let _ = processor.process([0.1, -0.1]);
    }
    assert!(
        !processor.settings_pending(),
        "queued retune did not settle after discontinuity"
    );

    let input = [0.1_f32, -0.1_f32];
    let actual = processor.process(input);
    let mut fresh = Processor::new(final_target);
    let expected = fresh.process(input);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-7,
            "discontinuity lost the queued final target: actual={actual:?} expected={expected:?}"
        );
    }
}

#[test]
fn discontinuity_preserves_limiter_attenuation_and_avoids_recovery_blast() {
    let settings = Settings::compile(&Profile::default(), 48_000).unwrap();
    let mut processor = Processor::new(settings);

    for _ in 0..4_096 {
        let _ = processor.process([4.0, -4.0]);
    }
    let before = processor.limiter_reduction_db();
    assert!(
        before > 5.0,
        "fixture did not engage limiter strongly: {before:.3} dB"
    );

    processor.reset_history();
    let after_reset = processor.limiter_reduction_db();
    assert!(
        (after_reset - before).abs() < 1e-9,
        "discontinuity released limiter attenuation: before={before:.3} dB after={after_reset:.3} dB"
    );

    let quiet = processor.process([0.1, -0.1]);
    let fresh = Processor::new(settings).process([0.1, -0.1]);
    assert!(
        quiet[0].abs() < fresh[0].abs() && quiet[1].abs() < fresh[1].abs(),
        "first post-discontinuity frame escaped preserved limiter attenuation: quiet={quiet:?} fresh={fresh:?}"
    );
}

#[test]
fn queued_target_after_single_frame_retune_does_not_skip_middle_target() {
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let middle_profile = Profile {
        stereo_width: 0.5,
        ..Profile::default()
    };
    let final_profile = Profile {
        stereo_width: 0.0,
        ..Profile::default()
    };
    let mut middle = Settings::compile(&middle_profile, 48_000).unwrap();
    let mut final_target = Settings::compile(&final_profile, 48_000).unwrap();
    middle.transition_frames = 1;
    final_target.transition_frames = 1;

    let input = [0.1_f32, -0.1_f32];
    let mut processor = Processor::new(base);
    processor.update(middle);
    processor.update(final_target);

    let first = processor.process(input);
    let mut fresh_middle = Processor::new(middle);
    let expected_middle = fresh_middle.process(input);
    for channel in 0..2 {
        assert!(
            (first[channel] - expected_middle[channel]).abs() < 1e-7,
            "queued target skipped the audible middle endpoint: first={first:?} expected={expected_middle:?}"
        );
    }
    assert!(
        processor.settings_pending(),
        "queued final target disappeared when the one-frame middle retune completed"
    );

    let second = processor.process(input);
    let mut fresh_final = Processor::new(final_target);
    let expected_final = fresh_final.process(input);
    for channel in 0..2 {
        assert!(
            (second[channel] - expected_final[channel]).abs() < 1e-7,
            "queued final target did not become audible on the following frame: second={second:?} expected={expected_final:?}"
        );
    }
    assert!(
        !processor.settings_pending(),
        "single-frame queued final target remained pending after its audible endpoint"
    );
}

#[test]
fn single_frame_level_match_toggle_aligns_audio_pending_and_makeup_telemetry() {
    let profile = Profile::default();
    let base_music = crate::music::MusicProfile {
        correction_preamp_db: -12.0,
        correction_source: Some("single-frame telemetry fixture".into()),
        adaptive: crate::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: false,
        ..crate::music::MusicProfile::default()
    };
    let mut target_music = base_music.clone();
    target_music.level_match = true;

    let base = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&base_music, 48_000)
        .unwrap();
    let mut target = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&target_music, 48_000)
        .unwrap();
    target.transition_frames = 1;

    let input = [0.05_f32, -0.04_f32];
    let mut processor = Processor::new(base);
    processor.update(target);
    let actual = processor.process(input);
    assert!(
        !processor.settings_pending(),
        "single-frame Level Match toggle remained pending after its audible endpoint"
    );

    let mut fresh = Processor::new(target);
    let expected = fresh.process(input);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-7,
            "Level Match audio endpoint lagged its one-frame transition: actual={actual:?} expected={expected:?}"
        );
    }
    let actual_makeup = processor.level_match_makeup_db();
    let expected_makeup = fresh.level_match_makeup_db();
    assert!(
        (actual_makeup - expected_makeup).abs() < 1e-9,
        "Level Match telemetry lagged the audible endpoint: actual={actual_makeup:.6} expected={expected_makeup:.6} dB"
    );
    assert!(
        actual_makeup > 10.0,
        "fixture did not expose meaningful static makeup: {actual_makeup:.3} dB"
    );
}

#[test]
fn queued_single_frame_level_match_telemetry_reports_each_audible_endpoint_in_order() {
    let profile = Profile::default();
    let make_music = |correction_preamp_db: f64, level_match: bool| crate::music::MusicProfile {
        correction_preamp_db,
        correction_source: Some("queued telemetry fixture".into()),
        adaptive: crate::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match,
        ..crate::music::MusicProfile::default()
    };
    let base_music = make_music(-6.0, false);
    let middle_music = make_music(-6.0, true);
    let final_music = make_music(-12.0, true);

    let base = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&base_music, 48_000)
        .unwrap();
    let mut middle = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&middle_music, 48_000)
        .unwrap();
    let mut final_target = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&final_music, 48_000)
        .unwrap();
    middle.transition_frames = 1;
    final_target.transition_frames = 1;

    let input = [0.05_f32, -0.04_f32];
    let mut expected_middle = Processor::new(middle);
    let _ = expected_middle.process(input);
    let middle_makeup = expected_middle.level_match_makeup_db();
    let mut expected_final = Processor::new(final_target);
    let _ = expected_final.process(input);
    let final_makeup = expected_final.level_match_makeup_db();
    assert!(
        middle_makeup > 5.0 && final_makeup > 11.0,
        "fixture did not create distinct makeup endpoints: middle={middle_makeup:.3} final={final_makeup:.3} dB"
    );

    let mut processor = Processor::new(base);
    processor.update(middle);
    processor.update(final_target);

    let _ = processor.process(input);
    let first_makeup = processor.level_match_makeup_db();
    assert!(
        (first_makeup - middle_makeup).abs() < 1e-9,
        "queued final target leaked into telemetry before becoming audible: first={first_makeup:.6} middle={middle_makeup:.6} dB"
    );
    assert!(processor.settings_pending());

    let _ = processor.process(input);
    let second_makeup = processor.level_match_makeup_db();
    assert!(
        (second_makeup - final_makeup).abs() < 1e-9,
        "final queued telemetry did not align with its audible frame: second={second_makeup:.6} final={final_makeup:.6} dB"
    );
    assert!(!processor.settings_pending());
}

#[test]
fn retune_crossfade_duration_is_sample_rate_invariant() {
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let base = Settings::compile(&Profile::default(), rate).unwrap();
        let target_profile = Profile {
            crossfeed: 0.3,
            ..Profile::default()
        };
        let target = Settings::compile(&target_profile, rate).unwrap();
        let mut processor = Processor::new(base);
        processor.update(target);
        let expected_frames = rate / 40;
        for frame in 0..expected_frames {
            assert!(
                processor.settings_pending(),
                "retune finished early at {rate} Hz on frame {frame}/{expected_frames}"
            );
            let _ = processor.process([0.05, -0.025]);
        }
        assert!(
            !processor.settings_pending(),
            "retune exceeded 25 ms at {rate} Hz after {expected_frames} frames"
        );
    }
}

#[test]
fn retunes_never_release_existing_limiter_attenuation() {
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let mut processor = Processor::new(base);
    for _ in 0..4_096 {
        let _ = processor.process([4.0, -4.0]);
    }
    let engaged = processor.limiter_reduction_db();
    assert!(
        engaged > 5.0,
        "fixture did not engage limiter: {engaged:.3} dB"
    );

    let first = Settings::compile(
        &Profile {
            stereo_width: 0.5,
            crossfeed: 0.25,
            ..Profile::default()
        },
        48_000,
    )
    .unwrap();
    let second = Settings::compile(
        &Profile {
            stereo_width: 1.5,
            crossfeed: 0.0,
            ..Profile::default()
        },
        48_000,
    )
    .unwrap();

    let before_first = processor.limiter_reduction_db();
    processor.update(first);
    assert!(
        (processor.limiter_reduction_db() - before_first).abs() < 1e-12,
        "starting a retune released limiter attenuation"
    );

    let _ = processor.process([0.1, -0.1]);
    let after_one_frame = processor.limiter_reduction_db();
    assert!(
        after_one_frame > 1.0,
        "first retune frame released limiter attenuation too aggressively: {after_one_frame:.3} dB"
    );

    let before_queued = processor.limiter_reduction_db();
    processor.update(second);
    assert!(
        (processor.limiter_reduction_db() - before_queued).abs() < 1e-12,
        "queueing a retune released limiter attenuation"
    );
}
