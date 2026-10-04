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
