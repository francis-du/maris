use super::*;

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

#[test]
fn crossfeed_preserves_mono_and_direct_high_frequency_level() {
    let profile = Profile {
        crossfeed: 0.3,
        ..Profile::default()
    };
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        for hz in [70.0, 1_000.0, 8_000.0] {
            let mut processor = Processor::new(Settings::compile(&profile, rate).unwrap());
            for i in 0..(rate as usize * 2) {
                let x = (0.1 * (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin()) as f32;
                let y = processor.process([x, x]);
                if i > rate as usize {
                    assert!(
                        (y[0] - x).abs() < 1e-6 && (y[1] - x).abs() < 1e-6,
                        "crossfeed colored mono at {hz} Hz/{rate} Hz: input={x} output={y:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn crossfeed_keeps_low_frequency_ratio_without_attenuating_direct_highs() {
    let amount = 0.3;
    let profile = Profile {
        crossfeed: amount,
        ..Profile::default()
    };
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let mut low = Processor::new(Settings::compile(&profile, rate).unwrap());
        let mut left_power = 0.0_f64;
        let mut right_power = 0.0_f64;
        for i in 0..(rate as usize * 2) {
            let x = (0.1 * (std::f64::consts::TAU * 70.0 * i as f64 / rate as f64).sin()) as f32;
            let y = low.process([x, 0.0]);
            if i > rate as usize {
                left_power += f64::from(y[0]).powi(2);
                right_power += f64::from(y[1]).powi(2);
            }
        }
        let ratio = (right_power / left_power).sqrt();
        assert!(
            (ratio - amount).abs() < 0.02,
            "crossfeed low-frequency cross-ear ratio changed at {rate} Hz: {ratio:.4}"
        );

        let mut high = Processor::new(Settings::compile(&profile, rate).unwrap());
        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for i in 0..(rate as usize * 2) {
            let x = (0.1 * (std::f64::consts::TAU * 8_000.0 * i as f64 / rate as f64).sin()) as f32;
            let y = high.process([x, 0.0]);
            if i > rate as usize {
                input_power += f64::from(x).powi(2);
                output_power += f64::from(y[0]).powi(2);
            }
        }
        let direct_delta_db = 10.0 * (output_power / input_power).log10();
        assert!(
            direct_delta_db.abs() < 0.15,
            "crossfeed attenuated direct high frequencies by {direct_delta_db:.3} dB at {rate} Hz"
        );
    }
}

#[test]
fn level_match_does_not_undo_explicit_balance() {
    let render = |balance: f64, level_match: bool| {
        let music = MusicProfile {
            balance,
            level_match,
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        let mut processor = Processor::new(
            Settings::compile(&Profile::default(), 48_000)
                .unwrap()
                .with_music(&music, 48_000)
                .unwrap(),
        );
        let mut power = [0.0_f64; 2];
        for i in 0..384_000 {
            let x = (0.08 * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin()) as f32;
            let output = processor.process([x, x]);
            if i > 288_000 {
                for channel in 0..2 {
                    power[channel] += f64::from(output[channel]).powi(2);
                }
            }
        }
        power
    };

    for balance in [-1.0, -0.75, -0.5, 0.5, 0.75, 1.0] {
        let unmatched = render(balance, false);
        let matched = render(balance, true);
        for channel in 0..2 {
            if unmatched[channel] <= 1e-12 {
                assert!(
                    matched[channel] <= 1e-12,
                    "Level Match revived a balance-muted channel at balance {balance:+.2}"
                );
                continue;
            }
            let delta_db = 10.0 * (matched[channel] / unmatched[channel]).log10();
            assert!(
                delta_db.abs() < 0.1,
                "Level Match changed explicit balance gain by {delta_db:.3} dB at balance {balance:+.2}, channel {channel}"
            );
        }
    }
}

#[test]
fn rapid_balance_changes_do_not_click_or_leave_makeup() {
    let settings = |balance: f64| {
        let music = MusicProfile {
            balance,
            level_match: true,
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        Settings::compile(&Profile::default(), 48_000)
            .unwrap()
            .with_music(&music, 48_000)
            .unwrap()
    };

    let mut processor = Processor::new(settings(0.0));
    let mut previous = [0.0_f32; 2];
    let mut maximum_step = 0.0_f32;
    for i in 0..16_000 {
        match i {
            2_000 => processor.update(settings(1.0)),
            3_000 => processor.update(settings(-1.0)),
            4_000 => processor.update(settings(0.75)),
            4_500 => processor.update(settings(-0.5)),
            5_000 => processor.update(settings(0.0)),
            _ => {}
        }
        let t = i as f64 / 48_000.0;
        let x = (0.08 * (std::f64::consts::TAU * 997.0 * t).sin()) as f32;
        let output = processor.process([x, x]);
        if i > 1 {
            maximum_step = maximum_step
                .max((output[0] - previous[0]).abs())
                .max((output[1] - previous[1]).abs());
        }
        previous = output;
    }
    assert!(
        maximum_step < 0.02,
        "rapid balance changes produced a click-sized sample step: {maximum_step}"
    );
    assert!(
        processor.level_match_makeup_db().abs() < 0.05,
        "rapid balance changes left stale Level Match makeup: {:.3} dB",
        processor.level_match_makeup_db()
    );
}

#[test]
fn level_match_convergence_time_is_sample_rate_invariant() {
    let mut observed_ms = Vec::new();
    let mut observed_target_db = Vec::new();
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let music = MusicProfile {
            highpass_hz: Some(120.0),
            level_match: true,
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        let settings = Settings::compile(&Profile::default(), rate)
            .unwrap()
            .with_music(&music, rate)
            .unwrap();

        let mut settled = Processor::new(settings);
        for i in 0..(rate * 4) {
            let x = (0.05 * (std::f64::consts::TAU * 70.0 * i as f64 / rate as f64).sin()) as f32;
            let _ = settled.process([x, x]);
        }
        let target_db = settled.level_match_makeup_db();
        observed_target_db.push(target_db);

        let mut processor = Processor::new(settings);
        let mut frames = 0_u32;
        while frames < rate * 4 {
            let x =
                (0.05 * (std::f64::consts::TAU * 70.0 * frames as f64 / rate as f64).sin()) as f32;
            let _ = processor.process([x, x]);
            frames += 1;
            if processor.level_match_makeup_db() >= target_db * 0.9 {
                break;
            }
        }
        assert!(
            frames < rate * 4,
            "Level Match did not converge at {rate} Hz"
        );
        observed_ms.push(frames as f64 * 1000.0 / f64::from(rate));
    }

    let target_min = observed_target_db
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let target_max = observed_target_db.iter().copied().fold(0.0_f64, f64::max);
    assert!(
        target_max - target_min < 0.02,
        "Level Match settled to different makeup across rates: {observed_target_db:?}"
    );

    let time_min = observed_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let time_max = observed_ms.iter().copied().fold(0.0_f64, f64::max);
    assert!(
        time_max - time_min < 0.2,
        "Level Match convergence time changed with sample rate: {observed_ms:?}"
    );
}
