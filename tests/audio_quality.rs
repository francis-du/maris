use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn sine_gain(profile: &Profile, music: &MusicProfile, hz: f64) -> f64 {
    let mut dsp = Processor::new(
        Settings::compile(profile, 48000)
            .unwrap()
            .with_music(music, 48000)
            .unwrap(),
    );
    let (mut input, mut output) = (0.0, 0.0);
    for i in 0..96000 {
        let x = (std::f64::consts::TAU * hz * i as f64 / 48000.0).sin() as f32 * 0.01;
        let y = dsp.process([x, x])[0];
        if i >= 48000 {
            input += (x as f64).powi(2);
            output += (y as f64).powi(2);
        }
    }
    10.0_f64 * (output / input).log10()
}

#[test]
fn disabled_music_is_transparent_even_with_nonzero_settings() {
    let music = MusicProfile {
        bass_db: 6.0,
        air_db: -3.0,
        correction_preamp_db: -12.0,
        enabled: false,
        ..MusicProfile::default()
    };
    let mut dsp = maris::music::Processor::new(music.compile(48000).unwrap());
    for i in 0..9600 {
        let x = (i as f64 * 0.19).sin() * 0.1;
        let y = dsp.process([x, -x]);
        assert!(
            (y[0] - x).abs() < 1e-10,
            "Disabled music still changes amplitude"
        );
        assert!((y[1] + x).abs() < 1e-10);
    }
}

#[test]
fn bass_and_air_reserve_shared_headroom_not_two_independent_cuts() {
    let eq = Profile::default();
    let music = MusicProfile {
        bass_db: 6.0,
        air_db: 3.0,
        level_match: false,
        ..MusicProfile::default()
    };
    let mid = sine_gain(&eq, &music, 1000.0);
    assert!(mid > -7.2, "Stacked attenuation wastes level: {mid:.3} dB");
    let bass = sine_gain(&eq, &music, 30.0);
    assert!(
        bass - mid > 5.5,
        "Bass change must survive the entire processing chain"
    );
}

#[test]
fn bypass_and_processing_share_static_safety_gain() {
    let music = MusicProfile {
        air_db: 1.0,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: false,
        ..MusicProfile::default()
    };
    let active = Profile::preset("bass").unwrap();
    let mut bypass = active.clone();
    bypass.bypass = true;
    let active_gain = Settings::compile(&active, 48000)
        .unwrap()
        .with_music(&music, 48000)
        .unwrap()
        .effective_preamp_db();
    let bypass_gain = Settings::compile(&bypass, 48000)
        .unwrap()
        .with_music(&music, 48000)
        .unwrap()
        .effective_preamp_db();
    assert!(
        (active_gain - bypass_gain).abs() < 1e-9,
        "Bypass and processing used different safety gains: {bypass_gain:.3} vs {active_gain:.3} dB"
    );
}

fn rms_gain(profile: &Profile, music: &MusicProfile) -> f64 {
    let mut processor = Processor::new(
        Settings::compile(profile, 48000)
            .unwrap()
            .with_music(music, 48000)
            .unwrap(),
    );
    let mut power = 0.0;
    for i in 0..384000 {
        let t = i as f64 / 48000.0;
        let x = (0.12 * (std::f64::consts::TAU * 220.0 * t).sin()
            + 0.08 * (std::f64::consts::TAU * 1800.0 * t).sin()
            + 0.04 * (std::f64::consts::TAU * 7600.0 * t).sin()) as f32;
        let y = processor.process([x, x])[0] as f64;
        if i >= 288000 {
            power += y * y;
        }
    }
    power
}

fn program_loudness_delta_db_at_rate(
    profile: &Profile,
    music: &MusicProfile,
    amplitude: f64,
    rate: u32,
) -> f64 {
    let mut processor = Processor::new(
        Settings::compile(profile, rate)
            .unwrap()
            .with_music(music, rate)
            .unwrap(),
    );
    let mut input_power = 0.0;
    let mut output_power = 0.0;
    // Four seconds covers multiple 1.5 s meter time constants while keeping the
    // adversarial preset matrix cheap enough to run on every native CI target.
    let total = rate as usize * 4;
    let measure_from = rate as usize * 3;
    for i in 0..total {
        let t = i as f64 / rate as f64;
        let x = amplitude
            * (0.55 * (std::f64::consts::TAU * 71.0 * t).sin()
                + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.15 * (std::f64::consts::TAU * 6833.0 * t).sin());
        let y = processor.process([x as f32, (-0.73 * x) as f32]);
        if i >= measure_from {
            input_power += x * x + (0.73 * x).powi(2);
            output_power += f64::from(y[0]).powi(2) + f64::from(y[1]).powi(2);
        }
    }
    10.0 * (output_power / input_power).log10()
}

fn program_loudness_delta_db(profile: &Profile, music: &MusicProfile, amplitude: f64) -> f64 {
    program_loudness_delta_db_at_rate(profile, music, amplitude, 48000)
}

fn integrated_loudness_delta_db(profile: &Profile, music: &MusicProfile) -> f64 {
    const RATE: u32 = 48_000;
    let mut processor = Processor::new(
        Settings::compile(profile, RATE)
            .unwrap()
            .with_music(music, RATE)
            .unwrap(),
    );
    let mut input = Vec::with_capacity(RATE as usize * 3 * 2);
    let mut output = Vec::with_capacity(RATE as usize * 3 * 2);
    for i in 0..RATE as usize * 4 {
        let t = i as f64 / RATE as f64;
        let left = 0.06
            * (0.58 * (std::f64::consts::TAU * 83.0 * t).sin()
                + 0.27 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.15 * (std::f64::consts::TAU * 7133.0 * t).sin());
        let right = 0.06
            * (0.51 * (std::f64::consts::TAU * 109.0 * t + 0.31).sin()
                + 0.31 * (std::f64::consts::TAU * 1499.0 * t + 0.77).sin()
                + 0.18 * (std::f64::consts::TAU * 8921.0 * t + 1.13).sin());
        let frame = [left as f32, right as f32];
        let processed = processor.process(frame);
        if i >= RATE as usize {
            input.extend_from_slice(&frame);
            output.extend_from_slice(&processed);
        }
    }
    let mut dry = ebur128::EbuR128::new(2, RATE, ebur128::Mode::I).unwrap();
    let mut wet = ebur128::EbuR128::new(2, RATE, ebur128::Mode::I).unwrap();
    dry.add_frames_f32(&input).unwrap();
    wet.add_frames_f32(&output).unwrap();
    wet.loudness_global().unwrap() - dry.loudness_global().unwrap()
}

#[test]
fn global_bypass_is_loudness_matched_to_music_processing() {
    let music = MusicProfile {
        air_db: 0.5,
        softness: 1.0,
        width: 1.5,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 1.0,
        },
        compressor: maris::music::Compressor {
            enabled: true,
            ..maris::music::Compressor::default()
        },
        reference: false,
        level_match: true,
        ..MusicProfile::default()
    };
    let active = Profile::default();
    let mut bypass = active.clone();
    bypass.bypass = true;
    let wet = rms_gain(&active, &music);
    let dry = rms_gain(&bypass, &music);
    let delta_db = 10.0 * (dry / wet).log10();
    assert!(
        delta_db.abs() < 0.35,
        "Bypass/processing loudness mismatch remains {delta_db:.3} dB"
    );
}

#[test]
fn level_match_tracks_the_original_program_not_the_already_attenuated_dry_path() {
    for eq in ["flat", "warm", "vocal", "bass", "soft", "clarity"] {
        for music_name in ["natural", "warm", "detail", "night", "long-listening"] {
            let profile = Profile::preset(eq).unwrap();
            let music = MusicProfile::preset(music_name).unwrap();
            let delta = program_loudness_delta_db(&profile, &music, 0.08);
            assert!(
                delta.abs() < 0.5,
                "{eq}/{music_name}: Maris changed settled program loudness by {delta:.3} dB"
            );
        }
    }
}

#[test]
fn level_match_is_close_in_ebu_r128_integrated_loudness_not_only_raw_power() {
    let cases = [
        (
            Profile::preset("bass").unwrap(),
            MusicProfile::preset("detail").unwrap(),
        ),
        (
            Profile::preset("soft").unwrap(),
            MusicProfile::preset("warm").unwrap(),
        ),
        (
            Profile::default(),
            MusicProfile {
                correction: vec![maris::tone::Filter {
                    kind: maris::tone::Kind::LowShelf,
                    frequency_hz: 110.0,
                    gain_db: 6.0,
                    q: std::f64::consts::FRAC_1_SQRT_2,
                }],
                correction_preamp_db: -10.0,
                correction_source: Some("EBU R128 parity fixture".into()),
                adaptive: maris::music::AdaptiveEq {
                    enabled: false,
                    strength: 0.0,
                },
                ..MusicProfile::default()
            },
        ),
    ];
    for (profile, music) in cases {
        let delta = integrated_loudness_delta_db(&profile, &music);
        assert!(
            delta.abs() < 0.8,
            "EBU R128 integrated loudness mismatch remains {delta:.3} LU"
        );
    }
}

#[test]
fn level_match_is_sample_rate_invariant() {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    for rate in [44100, 96000, 192000] {
        let delta = program_loudness_delta_db_at_rate(&profile, &music, 0.04, rate);
        assert!(
            delta.abs() < 0.5,
            "{rate} Hz level matching changed settled loudness by {delta:.3} dB"
        );
    }
}

#[test]
fn level_match_transition_to_deep_static_reserve_never_drops_the_program() {
    let profile = Profile::default();
    let baseline_music = MusicProfile {
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let target_music = MusicProfile {
        correction_preamp_db: -24.0,
        correction_source: Some("transition loudness fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let baseline = Settings::compile(&profile, 48000)
        .unwrap()
        .with_music(&baseline_music, 48000)
        .unwrap();
    let target = Settings::compile(&profile, 48000)
        .unwrap()
        .with_music(&target_music, 48000)
        .unwrap();
    let mut processor = Processor::new(baseline);
    for _ in 0..4096 {
        let _ = processor.process([0.05, -0.04]);
    }
    processor.update(target);
    let mut minimum_ratio = f32::INFINITY;
    for _ in 0..4800 {
        let output = processor.process([0.05, -0.04]);
        minimum_ratio = minimum_ratio.min((output[0] / 0.05).abs());
    }
    assert!(
        minimum_ratio > 0.8,
        "settings transition exposed static safety attenuation: min ratio {minimum_ratio:.3}"
    );
}

#[test]
fn level_match_preserves_an_explicit_user_preamp_instead_of_forcing_zero_db() {
    let mut profile = Profile::preset("bass").unwrap();
    profile.preamp_db = -6.0;
    let music = MusicProfile::preset("warm").unwrap();
    let delta = program_loudness_delta_db(&profile, &music, 0.08);
    assert!(
        (delta + 6.0).abs() < 0.5,
        "explicit -6 dB preamp was not preserved: measured {delta:.3} dB"
    );
}

#[test]
fn level_match_restores_static_reserve_from_the_first_program_frame() {
    let profile = Profile::default();
    let music = MusicProfile {
        correction_preamp_db: -24.0,
        correction_source: Some("startup parity fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = Processor::new(
        Settings::compile(&profile, 48000)
            .unwrap()
            .with_music(&music, 48000)
            .unwrap(),
    );
    for frame in [
        [0.12_f32, -0.08_f32],
        [-0.17_f32, 0.11_f32],
        [0.05_f32, 0.04_f32],
    ] {
        let output = processor.process(frame);
        for channel in 0..2 {
            assert!(
                (output[channel] - frame[channel]).abs() < 1e-4,
                "startup loudness reserve leaked before the matcher settled: input={} output={}",
                frame[channel],
                output[channel]
            );
        }
    }
}

#[test]
fn level_match_keeps_normal_program_levels_close_to_the_original_path() {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    for amplitude in [0.08, 0.25, 0.5] {
        let delta = program_loudness_delta_db(&profile, &music, amplitude);
        assert!(
            delta.abs() < 1.0,
            "amplitude {amplitude:.2}: settled Maris loudness differed by {delta:.3} dB"
        );
    }
}

#[test]
fn measured_level_match_recovers_large_device_correction_reserve_at_safe_program_levels() {
    let profile = Profile::default();
    for correction_preamp_db in [-10.0, -24.0, -30.0] {
        let music = MusicProfile {
            correction: vec![maris::tone::Filter {
                kind: maris::tone::Kind::LowShelf,
                frequency_hz: 110.0,
                gain_db: 6.0,
                q: std::f64::consts::FRAC_1_SQRT_2,
            }],
            correction_preamp_db,
            correction_source: Some("offline loudness fixture".into()),
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        let delta = program_loudness_delta_db(&profile, &music, 0.03);
        assert!(
            delta.abs() < 0.5,
            "{correction_preamp_db:.0} dB device-correction reserve leaked into listening loudness: {delta:.3} dB"
        );
    }
}

#[test]
fn output_switch_baseline_keeps_original_loudness_matching_when_the_user_enabled_it() {
    let profile = Profile::default();
    let music = MusicProfile {
        correction: vec![maris::tone::Filter {
            kind: maris::tone::Kind::LowShelf,
            frequency_hz: 110.0,
            gain_db: 6.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
        }],
        correction_preamp_db: -10.0,
        correction_source: Some("offline startup loudness fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let baseline = music.output_switch_baseline();
    assert!(baseline.level_match);
    let delta = program_loudness_delta_db(&profile, &baseline, 0.03);
    assert!(
        delta.abs() < 0.5,
        "startup baseline leaked correction reserve into loudness: {delta:.3} dB"
    );
}

#[test]
fn disabling_level_match_keeps_the_conservative_headroom_behavior() {
    let profile = Profile::default();
    let music = MusicProfile {
        bass_db: 6.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let delta = program_loudness_delta_db(&profile, &music, 0.08);
    assert!(
        delta < -1.0,
        "level_match=false unexpectedly restored original loudness: {delta:.3} dB"
    );
}

#[test]
fn original_loudness_makeup_never_bypasses_the_output_ceiling() {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile {
        bass_db: 6.0,
        presence_db: 3.0,
        air_db: 3.0,
        compressor: maris::music::Compressor {
            enabled: true,
            threshold_db: -24.0,
            ratio: 4.0,
            ..maris::music::Compressor::default()
        },
        ..MusicProfile::default()
    };
    let mut processor = Processor::new(
        Settings::compile(&profile, 48000)
            .unwrap()
            .with_music(&music, 48000)
            .unwrap(),
    );
    let mut peak = 0.0_f32;
    for i in 0..192000 {
        let t = i as f64 / 48000.0;
        let x = (0.86 * (std::f64::consts::TAU * 53.0 * t).sin()
            + 0.31 * (std::f64::consts::TAU * 1700.0 * t).sin()) as f32;
        for sample in processor.process([x, -x]) {
            peak = peak.max(sample.abs());
            assert!(sample.is_finite());
        }
    }
    assert!(
        peak <= 0.891252,
        "level makeup escaped the -1 dBFS ceiling: {peak}"
    );
}

#[test]
fn removing_deep_static_reserve_cannot_reuse_old_makeup_as_a_gain_blast() {
    let profile = Profile::default();
    let quiet = MusicProfile {
        correction_preamp_db: -24.0,
        correction_source: Some("transition reserve fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let normal = MusicProfile {
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let quiet_settings = Settings::compile(&profile, 48000)
        .unwrap()
        .with_music(&quiet, 48000)
        .unwrap();
    let normal_settings = Settings::compile(&profile, 48000)
        .unwrap()
        .with_music(&normal, 48000)
        .unwrap();
    let frame = [0.01_f32, -0.0073_f32];
    let mut processor = Processor::new(quiet_settings);
    for _ in 0..96000 {
        let output = processor.process(frame);
        assert!((output[0] - frame[0]).abs() < 2e-4);
    }
    processor.update(normal_settings);
    let mut peak = 0.0_f32;
    for _ in 0..24000 {
        for sample in processor.process(frame) {
            peak = peak.max(sample.abs());
        }
    }
    assert!(
        peak < 0.012,
        "removing static reserve reused stale makeup and amplified a quiet signal: peak={peak}"
    );
}

#[test]
fn settled_large_makeup_cannot_blast_after_a_sudden_level_step() {
    let profile = Profile::default();
    let music = MusicProfile {
        correction: vec![maris::tone::Filter {
            kind: maris::tone::Kind::LowShelf,
            frequency_hz: 110.0,
            gain_db: 6.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
        }],
        correction_preamp_db: -24.0,
        correction_source: Some("level-step adversarial fixture".into()),
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = Processor::new(
        Settings::compile(&profile, 48000)
            .unwrap()
            .with_music(&music, 48000)
            .unwrap(),
    );
    for i in 0..384000 {
        let t = i as f64 / 48000.0;
        let x = 0.01 * (std::f64::consts::TAU * 997.0 * t).sin();
        let _ = processor.process([x as f32, (-0.73 * x) as f32]);
    }
    for frame in [[0.95_f32, -0.95_f32], [-0.95_f32, 0.95_f32]] {
        for sample in processor.process(frame) {
            assert!(sample.is_finite());
            assert!(
                sample.abs() <= 0.891252,
                "settled makeup escaped the -1 dBFS ceiling after a level step: {sample}"
            );
        }
    }
}

#[test]
fn gain_only_identity_has_no_added_distortion_or_crosstalk() {
    let eq = Profile::default();
    let mut dsp = Processor::new(Settings::compile(&eq, 48000).unwrap());
    let gain = 10.0_f64.powf(eq.preamp_db / 20.0);
    let (mut signal, mut residual) = (0.0, 0.0);
    for i in 0..48000 {
        let x = (std::f64::consts::TAU * 997.0 * i as f64 / 48000.0).sin() as f32 * 0.2;
        let y = dsp.process([x, 0.0]);
        let reference = x as f64 * gain;
        signal += reference * reference;
        residual += (y[0] as f64 - reference).powi(2);
        assert_eq!(y[1], 0.0);
    }
    assert!(10.0_f64 * (residual / signal).log10() < -120.0);
}
