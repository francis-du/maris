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
fn spectral_content_change_during_retune_does_not_turn_level_match_into_pumping() {
    const RATE: u32 = 48_000;
    const WINDOW: usize = RATE as usize * 2 / 5;
    const STEP: usize = RATE as usize / 20;
    let profile = Profile::default();
    let old_music = MusicProfile {
        bass_db: 5.0,
        presence_db: -1.5,
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
    let new_music = MusicProfile {
        bass_db: -3.0,
        presence_db: 2.5,
        air_db: 3.0,
        highpass_hz: Some(120.0),
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
    let old = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&old_music, RATE)
        .unwrap();
    let next = Settings::compile(&profile, RATE)
        .unwrap()
        .with_music(&new_music, RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let (old, next) = old.share_transition_headroom(next);
    let mut processor = Processor::new(old);

    let bass_program = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.055
                * (0.72 * (std::f64::consts::TAU * 73.0 * t).sin()
                    + 0.20 * (std::f64::consts::TAU * 511.0 * t).sin()
                    + 0.08 * (std::f64::consts::TAU * 4200.0 * t).sin())) as f32,
            (0.052
                * (0.69 * (std::f64::consts::TAU * 91.0 * t + 0.3).sin()
                    + 0.22 * (std::f64::consts::TAU * 733.0 * t + 0.6).sin()
                    + 0.09 * (std::f64::consts::TAU * 5100.0 * t + 1.0).sin())) as f32,
        ]
    };
    let bright_program = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.055
                * (0.10 * (std::f64::consts::TAU * 103.0 * t).sin()
                    + 0.38 * (std::f64::consts::TAU * 1711.0 * t).sin()
                    + 0.52 * (std::f64::consts::TAU * 8903.0 * t).sin())) as f32,
            (0.052
                * (0.11 * (std::f64::consts::TAU * 131.0 * t + 0.2).sin()
                    + 0.36 * (std::f64::consts::TAU * 2137.0 * t + 0.7).sin()
                    + 0.53 * (std::f64::consts::TAU * 10103.0 * t + 1.1).sin())) as f32,
        ]
    };

    let mut index = 0usize;
    for _ in 0..RATE as usize * 4 {
        let _ = processor.process(bass_program(index));
        index += 1;
    }

    processor.update(next);
    let mut dry = Vec::with_capacity(RATE as usize * 3 * 2);
    let mut wet = Vec::with_capacity(RATE as usize * 3 * 2);
    let mut makeup = Vec::with_capacity(RATE as usize * 3);
    for frame_index in 0..RATE as usize * 3 {
        let input = if frame_index < RATE as usize / 20 {
            bass_program(index)
        } else {
            bright_program(index)
        };
        index += 1;
        let output = processor.process(input);
        assert!(output
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 0.891252));
        dry.extend_from_slice(&input);
        wet.extend_from_slice(&output);
        makeup.push(processor.level_match_makeup_db());
    }

    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    for start in (0..=RATE as usize * 3 - WINDOW).step_by(STEP) {
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

    let mut maximum_makeup_step = 0.0_f64;
    for pair in makeup.windows(2) {
        maximum_makeup_step = maximum_makeup_step.max((pair[1] - pair[0]).abs());
    }
    assert!(
        minimum_delta > -1.75 && maximum_delta < 1.75,
        "content + settings transition pumped program loudness: min={minimum_delta:.3} LU max={maximum_delta:.3} LU"
    );
    assert!(
        maximum_makeup_step < 0.08,
        "Level Match telemetry stepped too abruptly during content switch: {maximum_makeup_step:.4} dB/sample"
    );
}

#[test]
fn global_bypass_reenable_does_not_replay_outer_filter_history() {
    const RATE: u32 = 48_000;
    let active_profile = Profile::preset("bass").unwrap();
    let mut bypass_profile = active_profile.clone();
    bypass_profile.bypass = true;
    let active = Settings::compile(&active_profile, RATE)
        .unwrap()
        .with_transition_ms(RATE, 1);
    let bypass = Settings::compile(&bypass_profile, RATE)
        .unwrap()
        .with_transition_ms(RATE, 1);
    let mut processor = Processor::new(active);

    let _ = processor.process([0.8, -0.8]);
    processor.update(bypass);
    for _ in 0..RATE as usize / 1000 + 2 {
        let _ = processor.process([0.0, 0.0]);
    }
    for _ in 0..RATE as usize {
        assert_eq!(processor.process([0.0, 0.0]), [0.0, 0.0]);
    }

    processor.update(active);
    let mut peak = 0.0_f32;
    for _ in 0..RATE as usize / 1000 + 4 {
        for sample in processor.process([0.0, 0.0]) {
            peak = peak.max(sample.abs());
        }
    }
    assert!(
        peak < 1e-9,
        "global bypass re-enable replayed frozen outer filter/crossfeed history: peak={peak}"
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
    let mut reference = Processor::new(a);

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
        let input = frame(index);
        let _ = processor.process(input);
        let _ = reference.process(input);
        index += 1;
    }

    let mut previous_makeup = processor.level_match_makeup_db();
    let mut maximum_makeup_step = 0.0_f64;
    processor.update(b);
    for _ in 0..RATE as usize * 3 / 100 {
        let input = frame(index);
        let _ = processor.process(input);
        let makeup = processor.level_match_makeup_db();
        maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
        previous_makeup = makeup;
        let _ = reference.process(input);
        index += 1;
    }
    processor.update(a);

    let mut deviation_power = 0.0_f64;
    let mut reference_power = 0.0_f64;
    for _ in 0..RATE as usize / 10 {
        let input = frame(index);
        index += 1;
        let actual = processor.process(input);
        let makeup = processor.level_match_makeup_db();
        maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
        previous_makeup = makeup;
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
    assert!(
        maximum_makeup_step < 0.08,
        "rapid A→B→A reversal stepped Level Match telemetry by {maximum_makeup_step:.4} dB/sample"
    );
}

#[test]
fn rapid_effect_toggles_keep_level_match_bounded_and_telemetry_smooth() {
    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;
    let profile = Profile::default();
    let make = |compressor: bool, bass_assist: bool, adaptive: bool| {
        let mut music = MusicProfile {
            level_match: true,
            adaptive: AdaptiveEq {
                enabled: adaptive,
                strength: if adaptive { 1.0 } else { 0.0 },
            },
            compressor: Compressor {
                enabled: compressor,
                threshold_db: -30.0,
                ratio: 4.0,
                attack_ms: 2.0,
                release_ms: 180.0,
                ..Compressor::default()
            },
            ..MusicProfile::default()
        };
        music.bass_assist.enabled = bass_assist;
        music.bass_assist.amount = if bass_assist { 0.8 } else { 0.0 };
        music
    };
    let compile = |music: &MusicProfile| {
        Settings::compile(&profile, RATE)
            .unwrap()
            .with_music(music, RATE)
            .unwrap()
            .with_transition_ms(RATE, 120)
    };
    let base = compile(&make(false, false, false));
    let compressed = compile(&make(true, false, false));
    let bass = compile(&make(false, true, false));
    let dynamic = compile(&make(false, false, true));
    let mut processor = Processor::new(base);

    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.08
                * (0.48 * (std::f64::consts::TAU * 73.0 * t).sin()
                    + 0.32 * (std::f64::consts::TAU * 997.0 * t).sin()
                    + 0.20 * (std::f64::consts::TAU * 8_300.0 * t).sin())) as f32,
            (0.08
                * (0.45 * (std::f64::consts::TAU * 109.0 * t + 0.4).sin()
                    + 0.34 * (std::f64::consts::TAU * 1_499.0 * t + 0.8).sin()
                    + 0.21 * (std::f64::consts::TAU * 7_700.0 * t + 1.1).sin())) as f32,
        ]
    };

    let mut index = 0usize;
    for _ in 0..RATE as usize * 3 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    let sequence = [compressed, bass, dynamic, base, dynamic, compressed, base];
    let mut previous_makeup = processor.level_match_makeup_db();
    let mut maximum_makeup_step = 0.0_f64;
    let mut minimum_block_delta = f64::INFINITY;
    let mut maximum_block_delta = f64::NEG_INFINITY;

    for settings in sequence {
        processor.update(settings);
        for _ in 0..3 {
            let mut input_power = 0.0_f64;
            let mut output_power = 0.0_f64;
            for _ in 0..BLOCK {
                let input = frame(index);
                index += 1;
                let output = processor.process(input);
                for channel in 0..2 {
                    input_power += f64::from(input[channel]).powi(2);
                    output_power += f64::from(output[channel]).powi(2);
                    assert!(output[channel].is_finite());
                    assert!(output[channel].abs() <= 0.891_252);
                }
                let makeup = processor.level_match_makeup_db();
                maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
                previous_makeup = makeup;
            }
            let delta = 10.0 * (output_power / input_power.max(1e-30)).log10();
            minimum_block_delta = minimum_block_delta.min(delta);
            maximum_block_delta = maximum_block_delta.max(delta);
        }
    }

    assert!(
        minimum_block_delta > -2.0 && maximum_block_delta < 2.0,
        "rapid effect toggles escaped Level Match bounds: min={minimum_block_delta:.3} dB max={maximum_block_delta:.3} dB"
    );
    assert!(
        maximum_makeup_step < 0.08,
        "rapid effect toggles stepped Level Match telemetry by {maximum_makeup_step:.4} dB/sample"
    );
}

