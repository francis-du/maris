use maris::music::MusicProfile;

#[test]
fn compressor_reenable_starts_from_fresh_envelope_after_disable() {
    let mut enabled = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    enabled.compressor.enabled = true;
    enabled.compressor.threshold_db = -36.0;
    enabled.compressor.ratio = 8.0;
    enabled.compressor.attack_ms = 1.0;
    enabled.compressor.release_ms = 500.0;
    let mut disabled = enabled.clone();
    disabled.compressor.enabled = false;

    let enabled_settings = enabled.compile(48_000).unwrap();
    let disabled_settings = disabled.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(enabled_settings);
    for _ in 0..24_000 {
        let _ = processor.process([0.6, -0.6]);
    }

    processor = processor.retune(disabled_settings);
    for i in 0..24_000 {
        let x = 0.03 * (std::f64::consts::TAU * 700.0 * i as f64 / 48_000.0).sin();
        let y = processor.process([x, -x]);
        assert!((y[0] - x).abs() < 1e-10 && (y[1] + x).abs() < 1e-10);
    }

    processor = processor.retune(enabled_settings);
    let mut fresh = maris::music::Processor::new(enabled_settings);
    for i in 0..2_048 {
        let x = 0.04 * (std::f64::consts::TAU * 700.0 * i as f64 / 48_000.0).sin();
        let actual = processor.process([x, -x]);
        let expected = fresh.process([x, -x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-10,
                "re-enabled compressor replayed stale envelope: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn bass_assist_reenable_starts_from_fresh_filter_history_after_disable() {
    let mut enabled = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    enabled.bass_assist.enabled = true;
    enabled.bass_assist.amount = 1.0;
    let mut disabled = enabled.clone();
    disabled.bass_assist.enabled = false;

    let enabled_settings = enabled.compile(48_000).unwrap();
    let disabled_settings = disabled.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(enabled_settings);
    for i in 0..8_192 {
        let x = 0.4 * (std::f64::consts::TAU * 65.0 * i as f64 / 48_000.0).sin();
        let _ = processor.process([x, x]);
    }

    processor = processor.retune(disabled_settings);
    for i in 0..24_000 {
        let x = 0.05 * (std::f64::consts::TAU * 1_000.0 * i as f64 / 48_000.0).sin();
        let y = processor.process([x, x]);
        assert!((y[0] - x).abs() < 1e-10 && (y[1] - x).abs() < 1e-10);
    }

    processor = processor.retune(enabled_settings);
    let mut fresh = maris::music::Processor::new(enabled_settings);
    for i in 0..4_096 {
        let x = 0.08 * (std::f64::consts::TAU * 65.0 * i as f64 / 48_000.0).sin();
        let actual = processor.process([x, x]);
        let expected = fresh.process([x, x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-10,
                "re-enabled Bass Assist replayed stale filter history: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn dynamic_eq_reenable_starts_from_fresh_detector_history_after_disable() {
    let enabled = MusicProfile {
        softness: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 1.0,
        },
        ..MusicProfile::default()
    };
    let mut disabled = enabled.clone();
    disabled.adaptive.enabled = false;

    let enabled_settings = enabled.compile(48_000).unwrap();
    let disabled_settings = disabled.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(enabled_settings);
    for i in 0..48_000 {
        let x = 0.35 * (std::f64::consts::TAU * 8_500.0 * i as f64 / 48_000.0).sin();
        let _ = processor.process([x, x]);
    }

    processor = processor.retune(disabled_settings);
    for i in 0..24_000 {
        let x = 0.04 * (std::f64::consts::TAU * 900.0 * i as f64 / 48_000.0).sin();
        let y = processor.process([x, x]);
        assert!((y[0] - x).abs() < 1e-10 && (y[1] - x).abs() < 1e-10);
    }

    processor = processor.retune(enabled_settings);
    let mut fresh = maris::music::Processor::new(enabled_settings);
    for i in 0..4_096 {
        let x = 0.08 * (std::f64::consts::TAU * 8_500.0 * i as f64 / 48_000.0).sin();
        let actual = processor.process([x, x]);
        let expected = fresh.process([x, x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-9,
                "re-enabled Dynamic EQ replayed stale detector history: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}
