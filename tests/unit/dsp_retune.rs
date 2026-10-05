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
