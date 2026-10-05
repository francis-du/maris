use maris::music::MusicProfile;

#[test]
fn linked_compressor_preserves_stereo_ratio_under_one_sided_pressure() {
    let mut profile = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq { enabled: false, strength: 0.0 },
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
    assert!(compressed, "fixture never entered compressor gain reduction");
}

#[test]
fn dynamic_eq_applies_equal_transfer_to_proportional_stereo_channels() {
    let profile = MusicProfile {
        softness: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq { enabled: true, strength: 1.0 },
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