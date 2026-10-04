use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
    tone::{Filter, Kind},
};

#[test]
fn shelf_and_peak_coefficients_match_requested_response() {
    let low = Filter {
        kind: Kind::LowShelf,
        frequency_hz: 100.0,
        gain_db: 6.0,
        q: std::f64::consts::FRAC_1_SQRT_2,
    }
    .compile(48000)
    .unwrap();
    assert!((low.response_db(1.0, 48000) - 6.0).abs() < 0.01);
    assert!(low.response_db(10000.0, 48000).abs() < 0.01);
    let peak = Filter {
        kind: Kind::Peak,
        frequency_hz: 2300.0,
        gain_db: 3.0,
        q: 0.7,
    }
    .compile(48000)
    .unwrap();
    assert!((peak.response_db(2300.0, 48000) - 3.0).abs() < 1e-8);
    let high = Filter {
        kind: Kind::HighShelf,
        frequency_hz: 8000.0,
        gain_db: 3.0,
        q: std::f64::consts::FRAC_1_SQRT_2,
    }
    .compile(48000)
    .unwrap();
    assert!((high.response_db(23900.0, 48000) - 3.0).abs() < 0.01);
}
#[test]
fn correction_and_enhancements_remain_finite_and_limited() {
    for rate in [44100, 48000, 96000, 192000] {
        let music = MusicProfile {
            bass_db: 6.0,
            presence_db: 3.0,
            air_db: 3.0,
            width: 1.5,
            virtual_surround: 1.0,
            stereo_focus: 0.5,
            ..MusicProfile::default()
        };
        let mut dsp = Processor::new(
            Settings::compile(&Profile::default(), rate)
                .unwrap()
                .with_music(&music, rate)
                .unwrap(),
        );
        for i in 0..20000 {
            let x = if i == 500 {
                f32::NAN
            } else {
                (i as f32 * 0.037).sin() * 16.0
            };
            assert!(dsp
                .process([x, -x])
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 0.891251));
        }
    }
}
#[test]
fn ab_mean_square_match_does_not_boost_reference() {
    let mut enhanced = MusicProfile {
        bass_db: 4.0,
        air_db: -2.0,
        ..MusicProfile::default()
    };
    let mut wet = maris::music::Processor::new(enhanced.compile(48000).unwrap());
    enhanced.reference = true;
    let mut dry = maris::music::Processor::new(enhanced.compile(48000).unwrap());
    let mut a = 0.0;
    let mut b = 0.0;
    for i in 0..384000 {
        let t = i as f64 / 48000.0;
        let x = 0.08 * (t * 100.0 * std::f64::consts::TAU).sin()
            + 0.05 * (t * 8000.0 * std::f64::consts::TAU).sin();
        let w = wet.process([x, x]);
        let d = dry.process([x, x]);
        assert!(d[0].abs() <= x.abs() + 1e-12);
        if i > 336000 {
            a += w[0] * w[0];
            b += d[0] * d[0];
        }
    }
    assert!((10.0 * (a / b).log10()).abs() < 0.15);
}
#[test]
fn compressor_is_optional_and_reduces_loud_material() {
    let mut p = MusicProfile {
        level_match: false,
        ..MusicProfile::default()
    };
    let mut normal = maris::music::Processor::new(p.compile(48000).unwrap());
    p.compressor.enabled = true;
    p.compressor.threshold_db = -24.0;
    let mut limited = maris::music::Processor::new(p.compile(48000).unwrap());
    let mut before = 0.0;
    let mut after = 0.0;
    for i in 0..96000 {
        let x = (i as f64 * 0.13).sin() * 0.5;
        let a = normal.process([x, 0.0]);
        let b = limited.process([x, 0.0]);
        assert_eq!(b[1], 0.0);
        if i > 48000 {
            before += a[0] * a[0];
            after += b[0] * b[0];
        }
    }
    assert!(after < before * 0.5);
    assert!(!MusicProfile::default().compressor.enabled);
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

#[test]
fn reference_toggle_preserves_settled_level_match_history_for_the_same_signal_path() {
    let wet_profile = MusicProfile {
        highpass_hz: Some(180.0),
        level_match: true,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: maris::music::Compressor {
            enabled: false,
            ..maris::music::Compressor::default()
        },
        ..MusicProfile::default()
    };
    let mut reference_profile = wet_profile.clone();
    reference_profile.reference = true;

    let wet_settings = wet_profile.compile(48_000).unwrap();
    let reference_settings = reference_profile.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(wet_settings);

    let sample =
        |index: usize| 0.12 * (std::f64::consts::TAU * 80.0 * index as f64 / 48_000.0).sin();
    let mut index = 0usize;
    let mut settled_power = 0.0;
    for i in 0..48_000 * 4 {
        let x = sample(index);
        index += 1;
        let y = processor.process([x, x]);
        if i >= 48_000 * 3 {
            settled_power += y[0] * y[0] + y[1] * y[1];
        }
    }
    let settled_rms = (settled_power / (48_000.0 * 2.0)).sqrt();

    processor = processor.retune(reference_settings);
    let mut switched_power = 0.0;
    for _ in 0..4_800 {
        let x = sample(index);
        index += 1;
        let y = processor.process([x, x]);
        switched_power += y[0] * y[0] + y[1] * y[1];
    }
    let switched_rms = (switched_power / (4_800.0 * 2.0)).sqrt();
    let delta_db = 20.0 * (switched_rms / settled_rms).max(1e-12).log10();
    assert!(
        delta_db.abs() < 0.35,
        "Reference toggle discarded settled Level Match history and changed A/B level by {delta_db:.3} dB"
    );
}

#[test]
fn output_switch_baseline_preserves_correction_but_neutralizes_subjective_controls() {
    let mut profile = maris::music::MusicProfile::preset("warm").unwrap();
    profile.presence_db = 1.0;
    profile.air_db = 1.5;
    profile.width = 1.4;
    profile.balance = 0.2;
    profile.compressor.enabled = true;
    profile.adaptive.enabled = true;
    profile.bass_assist.enabled = true;
    profile.reference = true;
    profile.correction_preamp_db = -2.5;
    profile.correction_source = Some("fixture correction".into());
    profile.correction.push(maris::tone::Filter {
        kind: maris::tone::Kind::Peak,
        frequency_hz: 3000.0,
        gain_db: -2.0,
        q: 1.0,
    });

    let baseline = profile.output_switch_baseline();
    assert_eq!(baseline.correction, profile.correction);
    assert_eq!(baseline.correction_preamp_db, profile.correction_preamp_db);
    assert_eq!(baseline.correction_source, profile.correction_source);
    assert_eq!(baseline.bass_db, 0.0);
    assert_eq!(baseline.presence_db, 0.0);
    assert_eq!(baseline.air_db, 0.0);
    assert_eq!(baseline.width, 1.0);
    assert_eq!(baseline.balance, 0.0);
    assert_eq!(baseline.virtual_surround, 0.0);
    assert_eq!(baseline.stereo_focus, 0.0);
    assert!(!baseline.compressor.enabled);
    assert!(!baseline.adaptive.enabled);
    assert!(!baseline.bass_assist.enabled);
    assert!(!baseline.reference);
    baseline.validate().unwrap();
}

#[test]
fn invalid_music_settings_are_rejected() {
    let mut p = MusicProfile {
        bass_db: f64::NAN,
        ..MusicProfile::default()
    };
    assert!(p.compile(48000).is_err());
    p.bass_db = 0.0;
    p.width = 10.0;
    assert!(p.validate().is_err());
    p.width = 1.0;
    p.virtual_surround = 1.1;
    assert!(p.validate().is_err());
    p.virtual_surround = 0.0;
    p.stereo_focus = -0.1;
    assert!(p.validate().is_err());
    p.stereo_focus = 0.0;
    p.highpass_hz = Some(15000.0);
    assert!(p.validate().is_err());
}
#[test]
fn stereo_width_preserves_center_level() {
    let profile = MusicProfile {
        width: 1.5,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    let mut input = 0.0;
    let mut output = 0.0;
    for i in 0..96000 {
        let x = 0.1 * (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin();
        let y = processor.process([x, x]);
        if i >= 48000 {
            input += x * x;
            output += y[0] * y[0];
        }
    }
    let gain_db = 10.0 * (output / input).log10();
    assert!(
        gain_db.abs() < 0.02,
        "Stereo width attenuated centered content by {gain_db:.3} dB"
    );
}

#[test]
fn virtual_surround_keeps_mono_center_unchanged() {
    let profile = MusicProfile {
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    for i in 0..48000 {
        let x = 0.15 * (std::f64::consts::TAU * 440.0 * i as f64 / 48000.0).sin();
        let y = processor.process([x, x]);
        assert!((y[0] - x).abs() < 1e-10);
        assert!((y[1] - x).abs() < 1e-10);
    }
}

#[test]
fn virtual_surround_does_not_replay_stale_side_after_a_mono_passage() {
    let profile = MusicProfile {
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    for i in 0..2048 {
        let x = 0.2 * (std::f64::consts::TAU * 3500.0 * i as f64 / 48000.0).sin();
        let _ = processor.process([x, -x]);
    }
    for i in 0..4096 {
        let x = 0.1 * (std::f64::consts::TAU * 440.0 * i as f64 / 48000.0).sin();
        let y = processor.process([x, x]);
        assert!(
            (y[0] - y[1]).abs() < 1e-12,
            "Virtual 360 leaked stale Side energy into mono content"
        );
    }
}

#[test]
fn music_reenable_does_not_replay_filter_state_frozen_while_disabled() {
    let enabled = MusicProfile {
        bass_db: 6.0,
        air_db: -3.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut disabled = enabled.clone();
    disabled.enabled = false;

    let enabled_settings = enabled.compile(48_000).unwrap();
    let disabled_settings = disabled.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(enabled_settings);
    let _ = processor.process([0.8, -0.8]);

    processor = processor.retune(disabled_settings);
    for _ in 0..48_000 {
        assert_eq!(processor.process([0.0, 0.0]), [0.0, 0.0]);
    }

    processor = processor.retune(enabled_settings);
    let mut fresh = maris::music::Processor::new(enabled_settings);
    for _ in 0..256 {
        let actual = processor.process([0.0, 0.0]);
        let expected = fresh.process([0.0, 0.0]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-12,
                "re-enabled music stage replayed filter state frozen while disabled: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn virtual_surround_reenable_does_not_replay_state_frozen_while_disabled() {
    let enabled = MusicProfile {
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut disabled = enabled.clone();
    disabled.virtual_surround = 0.0;

    let enabled_settings = enabled.compile(48_000).unwrap();
    let disabled_settings = disabled.compile(48_000).unwrap();
    let mut processor = maris::music::Processor::new(enabled_settings);
    for i in 0..4_096 {
        let x = 0.2 * (std::f64::consts::TAU * 3_500.0 * i as f64 / 48_000.0).sin();
        let _ = processor.process([x, -x]);
    }

    processor = processor.retune(disabled_settings);
    for i in 0..48_000 {
        let x = 0.1 * (std::f64::consts::TAU * 440.0 * i as f64 / 48_000.0).sin();
        let y = processor.process([x, x]);
        assert!((y[0] - y[1]).abs() < 1e-12);
    }

    processor = processor.retune(enabled_settings);
    let mut fresh = maris::music::Processor::new(enabled_settings);
    for i in 0..2_048 {
        let x = 0.1 * (std::f64::consts::TAU * 440.0 * i as f64 / 48_000.0).sin();
        let actual = processor.process([x, x]);
        let expected = fresh.process([x, x]);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-10,
                "re-enabled Virtual 360 replayed filter state frozen while disabled: actual={} fresh={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn virtual_surround_decorrelates_side_but_preserves_center_sum() {
    let profile = MusicProfile {
        virtual_surround: 0.8,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    let mut changed = 0.0;
    for i in 0..96000 {
        let x = 0.12 * (std::f64::consts::TAU * 4000.0 * i as f64 / 48000.0).sin();
        let y = processor.process([x, -x]);
        assert!((y[0] + y[1]).abs() < 1e-10);
        if i > 48000 {
            changed += (y[0] - x).abs();
        }
    }
    assert!(changed > 10.0, "Virtual 360 did not alter the Side phase");
}

#[test]
fn stereo_focus_reduces_side_without_moving_mid() {
    let base = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut focused = base.clone();
    focused.stereo_focus = 1.0;
    let mut a = maris::music::Processor::new(base.compile(48000).unwrap());
    let mut b = maris::music::Processor::new(focused.compile(48000).unwrap());
    let mut dry = 0.0;
    let mut wet = 0.0;
    for i in 0..96000 {
        let x = 0.1 * (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin();
        let da = a.process([x, -x]);
        let fb = b.process([x, -x]);
        assert!((fb[0] + fb[1]).abs() < 1e-10);
        if i > 48000 {
            dry += da[0] * da[0];
            wet += fb[0] * fb[0];
        }
    }
    assert!(
        wet < dry * 0.08,
        "Stereo Focus did not sufficiently reduce Side energy"
    );
}

fn harmonic_amplitude(profile: &MusicProfile, input_hz: f64, measure_hz: f64) -> f64 {
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    let mut sin = 0.0;
    let mut cos = 0.0;
    let mut count = 0.0;
    for i in 0..144000 {
        let t = i as f64 / 48000.0;
        let x = 0.18 * (std::f64::consts::TAU * input_hz * t).sin();
        let y = processor.process([x, x])[0];
        if i >= 96000 {
            let phase = std::f64::consts::TAU * measure_hz * t;
            sin += y * phase.sin();
            cos += y * phase.cos();
            count += 1.0;
        }
    }
    2.0 * (sin * sin + cos * cos).sqrt() / count
}

#[test]
fn virtual_bass_generates_bounded_harmonics_only_for_low_frequency_content() {
    let off = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut on = off.clone();
    on.bass_assist.enabled = true;
    on.bass_assist.amount = 0.8;
    let dry_h3 = harmonic_amplitude(&off, 60.0, 180.0);
    let wet_h3 = harmonic_amplitude(&on, 60.0, 180.0);
    assert!(
        wet_h3 > dry_h3 + 0.0005,
        "Virtual bass did not generate a useful third harmonic"
    );

    let dry_2k = harmonic_amplitude(&off, 2000.0, 2000.0);
    let wet_2k = harmonic_amplitude(&on, 2000.0, 2000.0);
    let delta_db = 20.0 * (wet_2k / dry_2k).log10();
    assert!(
        delta_db.abs() < 0.03,
        "Virtual bass changed non-bass content by {delta_db:.3} dB"
    );
    assert!(!MusicProfile::default().bass_assist.enabled);
}

#[test]
fn music_processing_is_transparent_when_adaptive_is_off() {
    let mut profile = MusicProfile::default();
    profile.adaptive.enabled = false;
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    for i in 0..10000 {
        let x = (i as f64 * 0.1).sin() * 0.1;
        let y = processor.process([x, -x]);
        assert!((y[0] - x).abs() < 1e-10);
        assert!((y[1] + x).abs() < 1e-10);
    }
}

#[test]
fn softness_is_dynamic_instead_of_a_permanent_treble_shelf() {
    let profile = MusicProfile {
        softness: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 1.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    let mut input = 0.0;
    let mut output = 0.0;
    for i in 0..96000 {
        let x = 0.08 * (std::f64::consts::TAU * 10000.0 * i as f64 / 48000.0).sin();
        let y = processor.process([x, x]);
        if i >= 48000 {
            input += x * x;
            output += y[0] * y[0];
        }
    }
    let gain_db = 10.0 * (output / input).log10();
    assert!(
        gain_db.abs() < 0.02,
        "Softness still acts as a static shelf: {gain_db:.3} dB"
    );
}

fn adaptive_tone_gain(hz: f64, enabled: bool) -> (f64, f64) {
    let profile = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled,
            strength: 1.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = maris::music::Processor::new(profile.compile(48000).unwrap());
    let mut input = 0.0;
    let mut output = 0.0;
    for i in 0..144000 {
        let x = 0.1 * (std::f64::consts::TAU * hz * i as f64 / 48000.0).sin();
        let y = processor.process([x, x]);
        if i >= 96000 {
            input += x * x;
            output += y[0] * y[0];
        }
    }
    (
        10.0 * (output / input).log10(),
        processor.adaptive_reduction_db(),
    )
}

#[test]
fn adaptive_eq_reacts_to_harsh_band_without_boosting() {
    let (off, _) = adaptive_tone_gain(3200.0, false);
    let (on, reduction) = adaptive_tone_gain(3200.0, true);
    assert!(off.abs() < 0.01);
    assert!(on < -0.4, "Target band was not controlled: {on:.3} dB");
    assert!(on > -1.7, "Adaptive cut exceeded its bound: {on:.3} dB");
    assert!(reduction > 0.4 && reduction <= 1.5 + 1e-6);
}

#[test]
fn adaptive_eq_leaves_midband_mostly_alone() {
    let (on, reduction) = adaptive_tone_gain(900.0, true);
    assert!(on > -0.2, "Off-target tone changed too much: {on:.3} dB");
    assert!(
        reduction < 0.2,
        "Off-target detector over-triggered: {reduction:.3} dB"
    );
}

fn retune_meter_profile(level_match: bool, reference: bool) -> MusicProfile {
    MusicProfile {
        bass_db: 6.0,
        air_db: -2.0,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: maris::music::Compressor {
            enabled: false,
            ..maris::music::Compressor::default()
        },
        level_match,
        reference,
        ..MusicProfile::default()
    }
}

fn retune_meter_frame(index: usize) -> [f64; 2] {
    let t = index as f64 / 48_000.0;
    let envelope = if index % 24_000 < 12_000 { 0.18 } else { 0.035 };
    [
        envelope
            * (0.72 * (std::f64::consts::TAU * 83.0 * t).sin()
                + 0.28 * (std::f64::consts::TAU * 2100.0 * t).sin()),
        envelope
            * (0.64 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                + 0.36 * (std::f64::consts::TAU * 6900.0 * t + 0.7).sin()),
    ]
}

#[test]
fn retune_preserves_level_match_meter_across_reference_toggle() {
    let wet_settings = retune_meter_profile(true, false).compile(48_000).unwrap();
    let reference_settings = retune_meter_profile(true, true).compile(48_000).unwrap();
    let mut source = maris::music::Processor::new(wet_settings);
    let mut control = maris::music::Processor::new(reference_settings);
    for index in 0..144_000 {
        let frame = retune_meter_frame(index);
        let _ = source.process(frame);
        let _ = control.process(frame);
    }

    let mut retuned = source.retune(reference_settings);
    for index in 144_000..144_128 {
        let frame = retune_meter_frame(index);
        let actual = retuned.process(frame);
        let expected = control.process(frame);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-10,
                "reference toggle discarded settled Level Match history: actual={} expected={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn retune_preserves_meter_when_level_match_is_enabled() {
    let unmatched_settings = retune_meter_profile(false, false).compile(48_000).unwrap();
    let matched_settings = retune_meter_profile(true, false).compile(48_000).unwrap();
    let mut source = maris::music::Processor::new(unmatched_settings);
    let mut control = maris::music::Processor::new(matched_settings);
    for index in 0..144_000 {
        let frame = retune_meter_frame(index);
        let _ = source.process(frame);
        let _ = control.process(frame);
    }

    let mut retuned = source.retune(matched_settings);
    for index in 144_000..144_128 {
        let frame = retune_meter_frame(index);
        let actual = retuned.process(frame);
        let expected = control.process(frame);
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-10,
                "enabling Level Match discarded meter history: actual={} expected={}",
                actual[channel],
                expected[channel]
            );
        }
    }
}
