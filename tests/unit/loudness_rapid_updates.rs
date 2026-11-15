use crate::{
    dsp::{Processor, Settings},
    music::{AdaptiveEq, Compressor, MusicProfile},
    profile::Profile,
};

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

#[test]
fn effect_toggle_rate_does_not_reintroduce_level_match_zipper() {
    const RATE: u32 = 48_000;
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
                ..Compressor::default()
            },
            ..MusicProfile::default()
        };
        music.bass_assist.enabled = bass_assist;
        music.bass_assist.amount = if bass_assist { 0.8 } else { 0.0 };
        Settings::compile(&profile, RATE)
            .unwrap()
            .with_music(&music, RATE)
            .unwrap()
            .with_transition_ms(RATE, 120)
    };
    let settings = [
        make(false, false, false),
        make(true, false, false),
        make(false, true, false),
        make(false, false, true),
    ];
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.08
                * (0.50 * (std::f64::consts::TAU * 83.0 * t).sin()
                    + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                    + 0.20 * (std::f64::consts::TAU * 8_300.0 * t).sin())) as f32,
            (0.08
                * (0.47 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                    + 0.32 * (std::f64::consts::TAU * 1_499.0 * t + 0.7).sin()
                    + 0.21 * (std::f64::consts::TAU * 7_700.0 * t + 1.1).sin())) as f32,
        ]
    };

    for interval_ms in [1_u32, 2, 5, 10, 20] {
        let interval = (RATE as usize * interval_ms as usize / 1_000).max(1);
        let mut processor = Processor::new(settings[0]);
        let mut index = 0usize;
        for _ in 0..RATE as usize {
            let _ = processor.process(frame(index));
            index += 1;
        }
        let mut previous_makeup = processor.level_match_makeup_db();
        let mut maximum_makeup_step = 0.0_f64;
        let mut next = 1usize;
        for sample_index in 0..RATE as usize / 2 {
            if sample_index % interval == 0 {
                processor.update(settings[next]);
                next = (next + 1) % settings.len();
            }
            let output = processor.process(frame(index));
            index += 1;
            for sample in output {
                assert!(sample.is_finite());
                assert!(sample.abs() <= 0.891_252);
            }
            let makeup = processor.level_match_makeup_db();
            maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
            previous_makeup = makeup;
        }
        assert!(
            maximum_makeup_step < 0.08,
            "{interval_ms} ms effect updates stepped Level Match by {maximum_makeup_step:.4} dB/sample"
        );
    }
}

#[test]
fn rapid_effect_dezipper_is_sample_rate_invariant() {
    let profile = Profile::default();
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
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
                    ..Compressor::default()
                },
                ..MusicProfile::default()
            };
            music.bass_assist.enabled = bass_assist;
            music.bass_assist.amount = if bass_assist { 0.8 } else { 0.0 };
            Settings::compile(&profile, rate)
                .unwrap()
                .with_music(&music, rate)
                .unwrap()
                .with_transition_ms(rate, 120)
        };
        let settings = [
            make(false, false, false),
            make(true, false, false),
            make(false, true, false),
            make(false, false, true),
        ];
        let mut processor = Processor::new(settings[0]);
        let frame = |index: usize| {
            let t = index as f64 / rate as f64;
            [
                (0.08
                    * (0.52 * (std::f64::consts::TAU * 83.0 * t).sin()
                        + 0.28 * (std::f64::consts::TAU * 997.0 * t).sin()
                        + 0.20 * (std::f64::consts::TAU * 8_300.0 * t).sin()))
                    as f32,
                (0.08
                    * (0.49 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                        + 0.30 * (std::f64::consts::TAU * 1_499.0 * t + 0.7).sin()
                        + 0.21 * (std::f64::consts::TAU * 7_700.0 * t + 1.1).sin()))
                    as f32,
            ]
        };
        let mut index = 0usize;
        for _ in 0..rate as usize {
            let _ = processor.process(frame(index));
            index += 1;
        }
        let interval = (rate as usize / 1_000).max(1);
        let mut previous_makeup = processor.level_match_makeup_db();
        let mut maximum_makeup_step = 0.0_f64;
        let mut next = 1usize;
        for sample_index in 0..rate as usize / 4 {
            if sample_index % interval == 0 {
                processor.update(settings[next]);
                next = (next + 1) % settings.len();
            }
            let _ = processor.process(frame(index));
            index += 1;
            let makeup = processor.level_match_makeup_db();
            maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
            previous_makeup = makeup;
        }
        assert!(
            maximum_makeup_step < 0.08,
            "{rate} Hz rapid effect updates stepped Level Match by {maximum_makeup_step:.4} dB/sample"
        );
    }
}

#[test]
fn one_millisecond_effect_updates_do_not_pump_short_window_loudness() {
    const RATE: u32 = 48_000;
    const WINDOW: usize = 240;
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
                ..Compressor::default()
            },
            ..MusicProfile::default()
        };
        music.bass_assist.enabled = bass_assist;
        music.bass_assist.amount = if bass_assist { 0.8 } else { 0.0 };
        Settings::compile(&profile, RATE)
            .unwrap()
            .with_music(&music, RATE)
            .unwrap()
            .with_transition_ms(RATE, 120)
    };
    let settings = [
        make(false, false, false),
        make(true, false, false),
        make(false, true, false),
        make(false, false, true),
    ];
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.08
                * (0.52 * (std::f64::consts::TAU * 83.0 * t).sin()
                    + 0.28 * (std::f64::consts::TAU * 997.0 * t).sin()
                    + 0.20 * (std::f64::consts::TAU * 8_300.0 * t).sin())) as f32,
            (0.08
                * (0.49 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                    + 0.30 * (std::f64::consts::TAU * 1_499.0 * t + 0.7).sin()
                    + 0.21 * (std::f64::consts::TAU * 7_700.0 * t + 1.1).sin())) as f32,
        ]
    };
    let mut processor = Processor::new(settings[0]);
    let mut index = 0usize;
    for _ in 0..RATE as usize {
        let _ = processor.process(frame(index));
        index += 1;
    }

    let interval = RATE as usize / 1_000;
    let mut next = 1usize;
    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    for _ in 0..100 {
        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for offset in 0..WINDOW {
            let absolute = index + offset;
            if absolute.is_multiple_of(interval) {
                processor.update(settings[next]);
                next = (next + 1) % settings.len();
            }
            let input = frame(index);
            index += 1;
            let output = processor.process(input);
            for channel in 0..2 {
                input_power += f64::from(input[channel]).powi(2);
                output_power += f64::from(output[channel]).powi(2);
            }
        }
        let delta = 10.0 * (output_power / input_power.max(1e-30)).log10();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
    }
    assert!(
        minimum_delta > -2.0 && maximum_delta < 2.0,
        "1 ms effect updates pumped 5 ms loudness: min={minimum_delta:.3} dB max={maximum_delta:.3} dB"
    );
}
