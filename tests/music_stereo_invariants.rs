use maris::music::MusicProfile;

#[test]
fn linked_compressor_preserves_stereo_ratio_under_one_sided_pressure() {
    let mut profile = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    profile.compressor.enabled = true;
    profile.compressor.threshold_db = -30.0;
    profile.compressor.ratio = 6.0;
    profile.compressor.attack_ms = 1.0;
    profile.compressor.release_ms = 200.0;
    let mut processor = maris::music::Processor::new(profile.compile(48_000).unwrap());

    let mut compressed = false;
    for i in 0..48_000 {
        let x = 0.65 * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin();
        let input = [x, x * 0.2];
        let output = processor.process(input);
        if input[1].abs() > 1e-4 {
            let input_ratio = input[0] / input[1];
            let output_ratio = output[0] / output[1];
            assert!(
                (output_ratio - input_ratio).abs() < 1e-9,
                "linked compressor shifted stereo ratio: in={input_ratio} out={output_ratio}"
            );
        }
        compressed |= output[0].abs() + 1e-6 < input[0].abs();
    }
    assert!(
        compressed,
        "fixture never entered compressor gain reduction"
    );
}

#[test]
fn dynamic_eq_applies_equal_transfer_to_proportional_stereo_channels() {
    let profile = MusicProfile {
        softness: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 1.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48_000).unwrap());

    let mut observed_reduction = false;
    for i in 0..96_000 {
        let x = 0.35 * (std::f64::consts::TAU * 8_500.0 * i as f64 / 48_000.0).sin();
        let input = [x, x * 0.2];
        let output = processor.process(input);
        if output[1].abs() > 1e-6 {
            let ratio = output[0] / output[1];
            assert!(
                (ratio - 5.0).abs() < 1e-6,
                "Dynamic EQ shifted proportional stereo image: ratio={ratio}"
            );
        }
        observed_reduction |= processor.adaptive_reduction_db() > 0.05;
    }
    assert!(observed_reduction, "fixture never activated Dynamic EQ");
}

#[test]
fn bass_assist_preserves_pure_side_without_creating_center_energy() {
    let mut profile = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    profile.bass_assist.enabled = true;
    profile.bass_assist.amount = 1.0;
    let mut processor = maris::music::Processor::new(profile.compile(48_000).unwrap());

    let mut mid_power = 0.0_f64;
    let mut side_power = 0.0_f64;
    for i in 0..144_000 {
        let x = 0.2 * (std::f64::consts::TAU * 65.0 * i as f64 / 48_000.0).sin();
        let output = processor.process([x, -x]);
        if i >= 96_000 {
            let mid = (output[0] + output[1]) * 0.5;
            let side = (output[0] - output[1]) * 0.5;
            mid_power += mid * mid;
            side_power += side * side;
        }
    }
    let leakage_db = 10.0 * (mid_power.max(1e-30) / side_power.max(1e-30)).log10();
    assert!(
        leakage_db < -60.0,
        "Bass Assist created center energy from pure Side: {leakage_db:.2} dB"
    );
}

#[test]
fn bass_assist_preserves_proportional_stereo_pan() {
    let mut profile = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    profile.bass_assist.enabled = true;
    profile.bass_assist.amount = 1.0;
    let mut processor = maris::music::Processor::new(profile.compile(48_000).unwrap());

    let mut maximum_ratio_error = 0.0_f64;
    let mut measured = 0usize;
    for i in 0..144_000 {
        let x = 0.25 * (std::f64::consts::TAU * 65.0 * i as f64 / 48_000.0).sin();
        let output = processor.process([x, x * 0.2]);
        if i >= 96_000 && output[1].abs() > 1e-4 {
            maximum_ratio_error = maximum_ratio_error.max((output[0] / output[1] - 5.0).abs());
            measured += 1;
        }
    }
    assert!(measured > 1_000);
    assert!(
        maximum_ratio_error < 0.05,
        "Bass Assist shifted fixed stereo pan: max ratio error {maximum_ratio_error:.4}"
    );
}

#[test]
fn bass_assist_extreme_drive_stays_linked_and_bounded() {
    let base = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut enabled = base.clone();
    enabled.bass_assist.enabled = true;
    enabled.bass_assist.amount = 1.0;
    let mut dry = maris::music::Processor::new(base.compile(48_000).unwrap());
    let mut wet = maris::music::Processor::new(enabled.compile(48_000).unwrap());

    let mut maximum_ratio_error = 0.0_f64;
    let mut maximum_residual = 0.0_f64;
    for i in 0..144_000 {
        let x = 8.0 * (std::f64::consts::TAU * 65.0 * i as f64 / 48_000.0).sin();
        let frame = [x, x * 0.2];
        let reference = dry.process(frame);
        let output = wet.process(frame);
        assert!(output.iter().all(|sample| sample.is_finite()));
        if i >= 96_000 {
            if output[1].abs() > 1e-4 {
                maximum_ratio_error = maximum_ratio_error.max((output[0] / output[1] - 5.0).abs());
            }
            for channel in 0..2 {
                maximum_residual =
                    maximum_residual.max((output[channel] - reference[channel]).abs());
            }
        }
    }
    assert!(
        maximum_ratio_error < 0.05,
        "extreme Bass Assist drive shifted fixed stereo pan: {maximum_ratio_error:.4}"
    );
    assert!(
        maximum_residual < 0.8,
        "Bass Assist harmonic residual escaped linked drive bound: {maximum_residual:.4}"
    );
}
