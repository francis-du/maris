use crate::{
    dsp::{Processor, Settings},
    music::{AdaptiveEq, Compressor, MusicProfile},
    profile::Profile,
    tone::{Filter, Kind},
};

fn startup_transition_bounds(profile: &Profile, music: &MusicProfile) -> (f64, f64) {
    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;
    let baseline = Settings::compile(profile, RATE)
        .unwrap()
        .with_music(&music.output_switch_baseline(), RATE)
        .unwrap();
    let target = Settings::compile(profile, RATE)
        .unwrap()
        .with_music(music, RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let (baseline, target) = baseline.share_transition_headroom(target);
    let mut processor = Processor::new(baseline);
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        let left = 0.04
            * (0.55 * (std::f64::consts::TAU * 83.0 * t).sin()
                + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.15 * (std::f64::consts::TAU * 6833.0 * t).sin());
        let right = 0.04
            * (0.52 * (std::f64::consts::TAU * 109.0 * t + 0.2).sin()
                + 0.31 * (std::f64::consts::TAU * 1499.0 * t + 0.7).sin()
                + 0.17 * (std::f64::consts::TAU * 8921.0 * t + 1.1).sin());
        [left as f32, right as f32]
    };
    let mut index = 0usize;
    for _ in 0..RATE as usize * 4 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    processor.update(target);
    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    for _ in 0..50 {
        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for _ in 0..BLOCK {
            let input = frame(index);
            index += 1;
            let output = processor.process(input);
            input_power += f64::from(input[0]).powi(2) + f64::from(input[1]).powi(2);
            output_power += f64::from(output[0]).powi(2) + f64::from(output[1]).powi(2);
        }
        let delta = 10.0 * (output_power / input_power).log10();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
    }
    (minimum_delta, maximum_delta)
}

#[test]
fn production_startup_crossfade_never_exposes_a_large_loudness_jump() {
    let correction_boost = MusicProfile {
        bass_db: 4.0,
        presence_db: 2.0,
        air_db: 2.0,
        correction: vec![Filter {
            kind: Kind::LowShelf,
            frequency_hz: 110.0,
            gain_db: 6.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
        }],
        correction_preamp_db: -10.0,
        correction_source: Some("startup transition loudness fixture".into()),
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let cases = [
        (
            "correction-boost",
            Profile::preset("clarity").unwrap(),
            correction_boost,
        ),
        (
            "bass-detail",
            Profile::preset("bass").unwrap(),
            MusicProfile::preset("detail").unwrap(),
        ),
        (
            "soft-warm",
            Profile::preset("soft").unwrap(),
            MusicProfile::preset("warm").unwrap(),
        ),
    ];
    for (name, profile, music) in cases {
        let (minimum_delta, maximum_delta) = startup_transition_bounds(&profile, &music);
        assert!(
            minimum_delta > -1.5 && maximum_delta < 1.5,
            "{name}: 120 ms production startup crossfade changed 10 ms block loudness too much: min={minimum_delta:.3} dB max={maximum_delta:.3} dB"
        );
    }
}

#[test]
fn production_retune_between_opposite_spectral_profiles_stays_perceptually_level_matched() {
    const RATE: u32 = 48_000;
    const WINDOW: usize = RATE as usize * 2 / 5; // 400 ms BS.1770 momentary window
    const STEP: usize = RATE as usize / 10; // 100 ms
    let profile = Profile::default();
    let bass_heavy = MusicProfile {
        bass_db: 6.0,
        presence_db: -2.0,
        air_db: -2.0,
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let lean_bright = MusicProfile {
        bass_db: -6.0,
        presence_db: 3.0,
        air_db: 3.0,
        highpass_hz: Some(180.0),
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let first = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&bass_heavy, RATE)
        .unwrap();
    let second = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&lean_bright, RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let (first, second) = first.share_transition_headroom(second);
    let mut processor = Processor::new(first);
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.04
                * (0.62 * (std::f64::consts::TAU * 83.0 * t).sin()
                    + 0.23 * (std::f64::consts::TAU * 997.0 * t).sin()
                    + 0.15 * (std::f64::consts::TAU * 6833.0 * t).sin())) as f32,
            (0.04
                * (0.57 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                    + 0.25 * (std::f64::consts::TAU * 1499.0 * t + 0.8).sin()
                    + 0.18 * (std::f64::consts::TAU * 8921.0 * t + 1.2).sin())) as f32,
        ]
    };
    let mut index = 0usize;
    for _ in 0..RATE as usize * 5 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    processor.update(second);
    let mut dry = Vec::with_capacity(RATE as usize * 2 * 2);
    let mut wet = Vec::with_capacity(RATE as usize * 2 * 2);
    for _ in 0..RATE as usize * 2 {
        let input = frame(index);
        index += 1;
        let output = processor.process(input);
        assert!(output
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 0.891252));
        dry.extend_from_slice(&input);
        wet.extend_from_slice(&output);
    }

    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    for start in (0..=RATE as usize * 2 - WINDOW).step_by(STEP) {
        let end = start + WINDOW;
        let mut dry_meter = ebur128::EbuR128::new(2, RATE, ebur128::Mode::M).unwrap();
        let mut wet_meter = ebur128::EbuR128::new(2, RATE, ebur128::Mode::M).unwrap();
        dry_meter.add_frames_f32(&dry[start * 2..end * 2]).unwrap();
        wet_meter.add_frames_f32(&wet[start * 2..end * 2]).unwrap();
        let delta =
            wet_meter.loudness_momentary().unwrap() - dry_meter.loudness_momentary().unwrap();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
    }
    assert!(
        minimum_delta > -1.5 && maximum_delta < 1.5,
        "opposite spectral retune changed BS.1770 momentary loudness too much: min={minimum_delta:.3} LU max={maximum_delta:.3} LU"
    );
}

#[test]
fn queued_spectral_retunes_coalesce_without_loudness_step_or_stale_makeup() {
    const RATE: u32 = 48_000;
    const WINDOW: usize = RATE as usize * 2 / 5;
    const STEP: usize = RATE as usize / 10;
    let profile = Profile::default();
    let make = |bass_db, presence_db, air_db| MusicProfile {
        bass_db,
        presence_db,
        air_db,
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let a = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&make(5.0, -1.5, -1.5), RATE)
        .unwrap();
    let b = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&make(-4.0, 2.5, 2.0), RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let c = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&make(2.0, 1.0, -2.0), RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let (a, b) = a.share_transition_headroom(b);
    let (_, c) = a.share_transition_headroom(c);
    let mut processor = Processor::new(a);
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.05
                * (0.44 * (std::f64::consts::TAU * 97.0 * t).sin()
                    + 0.34 * (std::f64::consts::TAU * 911.0 * t).sin()
                    + 0.22 * (std::f64::consts::TAU * 6203.0 * t).sin())) as f32,
            (0.05
                * (0.41 * (std::f64::consts::TAU * 127.0 * t + 0.2).sin()
                    + 0.36 * (std::f64::consts::TAU * 1373.0 * t + 0.6).sin()
                    + 0.23 * (std::f64::consts::TAU * 8011.0 * t + 1.0).sin())) as f32,
        ]
    };
    let mut index = 0usize;
    for _ in 0..RATE as usize * 4 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    processor.update(b);
    for _ in 0..RATE as usize * 3 / 100 {
        let _ = processor.process(frame(index));
        index += 1;
    }
    processor.update(c);

    let mut dry = Vec::with_capacity(RATE as usize * 2 * 2);
    let mut wet = Vec::with_capacity(RATE as usize * 2 * 2);
    for _ in 0..RATE as usize * 2 {
        let input = frame(index);
        index += 1;
        let output = processor.process(input);
        assert!(output
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 0.891252));
        dry.extend_from_slice(&input);
        wet.extend_from_slice(&output);
    }

    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    for start in (0..=RATE as usize * 2 - WINDOW).step_by(STEP) {
        let end = start + WINDOW;
        let mut dry_meter = ebur128::EbuR128::new(2, RATE, ebur128::Mode::M).unwrap();
        let mut wet_meter = ebur128::EbuR128::new(2, RATE, ebur128::Mode::M).unwrap();
        dry_meter.add_frames_f32(&dry[start * 2..end * 2]).unwrap();
        wet_meter.add_frames_f32(&wet[start * 2..end * 2]).unwrap();
        let delta =
            wet_meter.loudness_momentary().unwrap() - dry_meter.loudness_momentary().unwrap();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
    }
    assert!(
        minimum_delta > -1.5 && maximum_delta < 1.5,
        "queued retunes changed BS.1770 momentary loudness too much: min={minimum_delta:.3} LU max={maximum_delta:.3} LU"
    );
}

#[test]
fn explicit_preamp_change_remains_intentional_through_level_matched_transition() {
    const RATE: u32 = 48_000;
    const BLOCK: usize = RATE as usize / 100;
    let before_profile = Profile {
        preamp_db: 0.0,
        ..Profile::default()
    };
    let mut after_profile = before_profile.clone();
    after_profile.preamp_db = -6.0;
    let music = MusicProfile {
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let before = Settings::compile(&before_profile, RATE)
        .unwrap()
        .with_music(&music, RATE)
        .unwrap();
    let after = Settings::compile(&after_profile, RATE)
        .unwrap()
        .with_music(&music, RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let mut processor = Processor::new(before);
    let sample = |index: usize| {
        let t = index as f64 / RATE as f64;
        (0.08
            * (0.6 * (std::f64::consts::TAU * 337.0 * t).sin()
                + 0.4 * (std::f64::consts::TAU * 2441.0 * t).sin())) as f32
    };
    let mut index = 0usize;
    for _ in 0..RATE as usize * 3 {
        let x = sample(index);
        index += 1;
        let _ = processor.process([x, -0.8 * x]);
    }

    processor.update(after);
    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    let mut final_delta = 0.0;
    for block in 0..80 {
        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for _ in 0..BLOCK {
            let x = sample(index);
            index += 1;
            let input = [x, -0.8 * x];
            let output = processor.process(input);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891252));
            input_power += f64::from(input[0]).powi(2) + f64::from(input[1]).powi(2);
            output_power += f64::from(output[0]).powi(2) + f64::from(output[1]).powi(2);
        }
        let delta = 10.0 * (output_power / input_power).log10();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
        if block >= 60 {
            final_delta += delta / 20.0;
        }
    }
    assert!(
        maximum_delta < 0.5 && minimum_delta > -6.75,
        "explicit preamp transition overshot its intended range: min={minimum_delta:.3} dB max={maximum_delta:.3} dB"
    );
    assert!(
        (final_delta + 6.0).abs() < 0.35,
        "Level Match erased or distorted an explicit -6 dB preamp request: settled={final_delta:.3} dB"
    );
}

#[test]
fn rapid_reversal_does_not_finish_an_obsolete_loudness_target_first() {
    const RATE: u32 = 48_000;
    let profile = Profile::default();
    let make = |bass_db, air_db| MusicProfile {
        bass_db,
        air_db,
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        compressor: Compressor {
            enabled: false,
            ..Compressor::default()
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let a = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&make(0.0, 0.0), RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let b = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&make(6.0, -3.0), RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let (a, b) = a.share_transition_headroom(b);
    let mut processor = Processor::new(a);

    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.05
                * (0.55 * (std::f64::consts::TAU * 83.0 * t).sin()
                    + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                    + 0.15 * (std::f64::consts::TAU * 6833.0 * t).sin())) as f32,
            (0.05
                * (0.52 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                    + 0.31 * (std::f64::consts::TAU * 1499.0 * t + 0.7).sin()
                    + 0.17 * (std::f64::consts::TAU * 8921.0 * t + 1.1).sin())) as f32,
        ]
    };
    let mut index = 0usize;
    for _ in 0..RATE as usize * 3 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    processor.update(b);
    for _ in 0..RATE as usize * 3 / 100 {
        let _ = processor.process(frame(index));
        index += 1;
    }
    processor.update(a);

    let mut reference = Processor::new(a);
    let mut deviation_power = 0.0_f64;
    let mut reference_power = 0.0_f64;
    for _ in 0..RATE as usize / 10 {
        let input = frame(index);
        index += 1;
        let actual = processor.process(input);
        let expected = reference.process(input);
        for channel in 0..2 {
            deviation_power += f64::from(actual[channel] - expected[channel]).powi(2);
            reference_power += f64::from(expected[channel]).powi(2);
        }
    }
    let error_db = 10.0 * (deviation_power / reference_power.max(1e-30)).log10();
    assert!(
        error_db < -24.0,
        "rapid A→B→A reversal kept obsolete B audible too long: residual error {error_db:.2} dB"
    );
}

