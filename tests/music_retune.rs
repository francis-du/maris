use maris::music::MusicProfile;

#[test]
fn retune_resets_unchanged_downstream_filter_state_when_upstream_filter_changes() {
    let filter = |frequency_hz, gain_db| maris::tone::Filter {
        kind: maris::tone::Kind::Peak,
        frequency_hz,
        gain_db,
        q: 2.0,
    };
    let base = MusicProfile {
        correction: vec![filter(500.0, 6.0), filter(4_000.0, 6.0)],
        correction_source: Some("serial-state fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: false,
        ..MusicProfile::default()
    };
    let mut target = base.clone();
    target.correction[0].gain_db = -6.0;

    let mut trained = maris::music::Processor::new(base.compile(48_000).unwrap());
    let _ = trained.process([0.5, 0.5]);
    let target_settings = target.compile(48_000).unwrap();
    let mut retuned = trained.retune(target_settings);
    let mut fresh = maris::music::Processor::new(target_settings);
    let actual = retuned.process([0.0, 0.0]);
    let expected = fresh.process([0.0, 0.0]);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-12,
            "unchanged downstream biquad leaked stale state after upstream retune: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_does_not_carry_compressor_envelope_into_a_different_compressor() {
    let mut old = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    old.compressor.enabled = true;
    old.compressor.threshold_db = -48.0;
    old.compressor.ratio = 8.0;
    old.compressor.attack_ms = 1.0;
    old.compressor.release_ms = 500.0;

    let mut next = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    next.compressor.enabled = true;
    next.compressor.threshold_db = -6.0;
    next.compressor.ratio = 2.0;

    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for _ in 0..48_000 {
        let _ = trained.process([0.5, 0.5]);
    }

    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    let frame = [0.05, -0.05];
    let actual = retuned.process(frame);
    let expected = fresh.process(frame);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-10,
            "new compressor inherited stale gain reduction: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_does_not_carry_compressor_envelope_across_upstream_tone_change() {
    let mut old = MusicProfile {
        bass_db: 6.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    old.compressor.enabled = true;
    old.compressor.threshold_db = -24.0;
    old.compressor.ratio = 4.0;
    old.compressor.attack_ms = 1.0;
    old.compressor.release_ms = 500.0;

    let mut next = old.clone();
    next.bass_db = -6.0;
    next.air_db = 3.0;

    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for _ in 0..48_000 {
        let _ = trained.process([0.5, 0.5]);
    }

    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    let frame = [0.05, -0.05];
    let actual = retuned.process(frame);
    let expected = fresh.process(frame);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-8,
            "same compressor inherited stale envelope across a changed upstream signal path: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_does_not_carry_adaptive_reduction_into_changed_dynamic_eq() {
    let old = MusicProfile {
        softness: 1.0,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 1.0,
        },
        level_match: false,
        ..MusicProfile::default()
    };
    let next = MusicProfile {
        softness: 0.0,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 0.15,
        },
        level_match: false,
        ..MusicProfile::default()
    };

    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for i in 0..96_000 {
        let x = 0.35 * (std::f64::consts::TAU * 8_500.0 * i as f64 / 48_000.0).sin();
        let _ = trained.process([x, x]);
    }

    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    let frame = [0.08, 0.08];
    let actual = retuned.process(frame);
    let expected = fresh.process(frame);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-8,
            "changed Dynamic EQ inherited stale detector reduction: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_resets_adaptive_cut_state_when_upstream_tone_changes() {
    let adaptive = maris::music::AdaptiveEq {
        enabled: true,
        strength: 1.0,
    };
    let old = MusicProfile {
        bass_db: 6.0,
        presence_db: -2.0,
        air_db: 3.0,
        adaptive,
        level_match: false,
        ..MusicProfile::default()
    };
    let next = MusicProfile {
        bass_db: -6.0,
        presence_db: 3.0,
        air_db: -3.0,
        adaptive,
        level_match: false,
        ..MusicProfile::default()
    };
    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for i in 0..96_000 {
        let t = i as f64 / 48_000.0;
        let x = 0.3
            * (0.45 * (std::f64::consts::TAU * 95.0 * t).sin()
                + 0.55 * (std::f64::consts::TAU * 8_500.0 * t).sin());
        let _ = trained.process([x, -0.8 * x]);
    }
    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    for i in 0..128 {
        let t = i as f64 / 48_000.0;
        let x = 0.04 * (std::f64::consts::TAU * 1_200.0 * t).sin();
        let actual = retuned.process([x, x]);
        let expected = fresh.process([x, x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-7,
                "Dynamic EQ cut state leaked across upstream tone retune: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn retune_resets_bass_assist_state_when_its_dry_gain_changes() {
    let bass_assist = maris::music::BassAssist {
        enabled: true,
        amount: 1.0,
    };
    let old = MusicProfile {
        bass_assist,
        correction_preamp_db: 0.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let next = MusicProfile {
        bass_assist,
        correction_preamp_db: -12.0,
        correction_source: Some("gain-change fixture".into()),
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for i in 0..48_000 {
        let t = i as f64 / 48_000.0;
        let x = 0.35 * (std::f64::consts::TAU * 72.0 * t).sin();
        let _ = trained.process([x, x]);
    }
    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    for i in 0..256 {
        let t = i as f64 / 48_000.0;
        let x = 0.02 * (std::f64::consts::TAU * 440.0 * t).sin();
        let actual = retuned.process([x, x]);
        let expected = fresh.process([x, x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-8,
                "Bass Assist inherited filter history measured at a different dry gain: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn retune_resets_virtual_surround_state_when_upstream_wet_path_changes() {
    let old = MusicProfile {
        bass_db: 6.0,
        air_db: -3.0,
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let next = MusicProfile {
        bass_db: -6.0,
        air_db: 3.0,
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut trained = maris::music::Processor::new(old.compile(48_000).unwrap());
    for i in 0..48_000 {
        let t = i as f64 / 48_000.0;
        let x = 0.25 * (std::f64::consts::TAU * 3_700.0 * t).sin();
        let _ = trained.process([x, -x]);
    }
    let settings = next.compile(48_000).unwrap();
    let mut retuned = trained.retune(settings);
    let mut fresh = maris::music::Processor::new(settings);
    for i in 0..256 {
        let t = i as f64 / 48_000.0;
        let x = 0.02 * (std::f64::consts::TAU * 1_100.0 * t).sin();
        let actual = retuned.process([x, -0.7 * x]);
        let expected = fresh.process([x, -0.7 * x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-8,
                "Virtual 360 inherited all-pass history from a different upstream wet path: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}
