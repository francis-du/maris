use maris::music::MusicProfile;

fn profile(reference: bool) -> MusicProfile {
    let mut profile = MusicProfile {
        bass_db: 3.0,
        presence_db: -1.0,
        air_db: 2.0,
        softness: 0.8,
        width: 1.3,
        balance: 0.2,
        virtual_surround: 0.7,
        stereo_focus: 0.25,
        level_match: true,
        reference,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 0.8,
        },
        ..MusicProfile::default()
    };
    profile.bass_assist.enabled = true;
    profile.bass_assist.amount = 0.6;
    profile.compressor.enabled = true;
    profile.compressor.threshold_db = -24.0;
    profile.compressor.ratio = 3.0;
    profile.compressor.attack_ms = 4.0;
    profile.compressor.release_ms = 180.0;
    profile
}

fn frame(index: usize) -> [f64; 2] {
    let t = index as f64 / 48_000.0;
    [
        0.16 * (0.42 * (std::f64::consts::TAU * 67.0 * t).sin()
            + 0.33 * (std::f64::consts::TAU * 997.0 * t).sin()
            + 0.25 * (std::f64::consts::TAU * 8_500.0 * t).sin()),
        0.16 * (0.39 * (std::f64::consts::TAU * 83.0 * t + 0.3).sin()
            + 0.35 * (std::f64::consts::TAU * 1_499.0 * t + 0.7).sin()
            + 0.26 * (std::f64::consts::TAU * 7_700.0 * t + 1.1).sin()),
    ]
}

#[test]
fn reference_round_trip_preserves_all_wet_state() {
    let wet = profile(false).compile(48_000).unwrap();
    let reference = profile(true).compile(48_000).unwrap();
    let mut toggled = maris::music::Processor::new(wet);
    let mut control = maris::music::Processor::new(wet);
    let mut index = 0usize;

    for _ in 0..144_000 {
        let input = frame(index);
        index += 1;
        let _ = toggled.process(input);
        let _ = control.process(input);
    }

    toggled = toggled.retune(reference);
    for _ in 0..96_000 {
        let input = frame(index);
        index += 1;
        let _ = toggled.process(input);
        let _ = control.process(input);
    }

    toggled = toggled.retune(wet);
    let mut maximum_error = 0.0_f64;
    for _ in 0..8_192 {
        let input = frame(index);
        index += 1;
        let actual = toggled.process(input);
        let expected = control.process(input);
        for channel in 0..2 {
            maximum_error = maximum_error.max((actual[channel] - expected[channel]).abs());
        }
    }
    assert!(
        maximum_error < 1e-9,
        "Reference round-trip lost wet DSP state: max sample error {maximum_error:e}"
    );
}
