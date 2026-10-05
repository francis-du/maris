use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn music(correction_preamp_db: f64) -> MusicProfile {
    MusicProfile {
        correction_preamp_db,
        correction_source: (correction_preamp_db < 0.0)
            .then(|| "device-switch-regression".into()),
        level_match: true,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    }
}

#[test]
fn bidirectional_output_rebind_keeps_loudness_and_makeup_telemetry_smooth() {
    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;

    let eq = Profile::default();
    let deep = Settings::compile(&eq, RATE)
        .unwrap()
        .with_music(&music(-24.0), RATE)
        .unwrap();
    let flat = Settings::compile(&eq, RATE)
        .unwrap()
        .with_music(&music(0.0), RATE)
        .unwrap();
    let mut processor = Processor::new(deep);
    let frame = |index: usize| {
        let t = index as f64 / RATE as f64;
        [
            (0.05
                * (0.55 * (std::f64::consts::TAU * 83.0 * t).sin()
                    + 0.45 * (std::f64::consts::TAU * 1_900.0 * t).sin()))
                as f32,
            (0.05
                * (0.52 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                    + 0.48 * (std::f64::consts::TAU * 2_700.0 * t + 0.7).sin()))
                as f32,
        ]
    };

    let mut index = 0usize;
    for _ in 0..RATE as usize * 3 {
        let _ = processor.process(frame(index));
        index += 1;
    }

    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    let mut maximum_makeup_step = 0.0_f64;
    let mut previous_makeup = processor.level_match_makeup_db();

    for target in [flat, deep, flat, deep] {
        processor.update(target);
        for _ in 0..20 {
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
                maximum_makeup_step =
                    maximum_makeup_step.max((makeup - previous_makeup).abs());
                previous_makeup = makeup;
            }
            let delta = 10.0 * (output_power / input_power.max(1e-30)).log10();
            minimum_delta = minimum_delta.min(delta);
            maximum_delta = maximum_delta.max(delta);
        }
    }

    assert!(
        minimum_delta > -1.5 && maximum_delta < 1.5,
        "output rebind changed 10 ms loudness too much: min={minimum_delta:.3} dB max={maximum_delta:.3} dB"
    );
    assert!(
        maximum_makeup_step < 0.08,
        "output rebind stepped Level Match telemetry by {maximum_makeup_step:.4} dB/sample"
    );
}


fn maximum_rebind_makeup_step(rate: u32) -> f64 {
    let eq = Profile::default();
    let deep = Settings::compile(&eq, rate)
        .unwrap()
        .with_music(&music(-24.0), rate)
        .unwrap();
    let flat = Settings::compile(&eq, rate)
        .unwrap()
        .with_music(&music(0.0), rate)
        .unwrap();
    let mut processor = Processor::new(deep);
    for i in 0..rate as usize * 2 {
        let x =
            (0.05 * (std::f64::consts::TAU * 997.0 * i as f64 / rate as f64).sin()) as f32;
        let _ = processor.process([x, x]);
    }

    let mut maximum_step = 0.0_f64;
    let mut previous = processor.level_match_makeup_db();
    for target in [flat, deep] {
        processor.update(target);
        for i in 0..rate as usize / 10 {
            let x =
                (0.05 * (std::f64::consts::TAU * 997.0 * i as f64 / rate as f64).sin()) as f32;
            let _ = processor.process([x, x]);
            let makeup = processor.level_match_makeup_db();
            maximum_step = maximum_step.max((makeup - previous).abs());
            previous = makeup;
        }
    }
    maximum_step
}

#[test]
fn output_rebind_makeup_dezipper_is_sample_rate_invariant() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let maximum_step = maximum_rebind_makeup_step(rate);
        assert!(
            maximum_step < 0.03,
            "{rate} Hz output rebind stepped Level Match telemetry by {maximum_step:.5} dB/sample"
        );
    }
}


#[test]
fn output_rebind_preserves_limiter_safety_state() {
    const RATE: u32 = 48_000;

    let eq = Profile::default();
    let deep = Settings::compile(&eq, RATE)
        .unwrap()
        .with_music(&music(-24.0), RATE)
        .unwrap();
    let flat = Settings::compile(&eq, RATE)
        .unwrap()
        .with_music(&music(0.0), RATE)
        .unwrap();
    let mut processor = Processor::new(deep);

    for i in 0..24_000 {
        let x = (4.0 * (std::f64::consts::TAU * 997.0 * i as f64 / RATE as f64).sin()) as f32;
        let output = processor.process([x, -x]);
        assert!(output.iter().all(|sample| sample.abs() <= 0.891_252));
    }
    assert!(
        processor.limiter_reduction_db() > 5.0,
        "fixture did not engage limiter strongly"
    );

    let mut previous_makeup = processor.level_match_makeup_db();
    let mut maximum_makeup_step = 0.0_f64;
    for target in [flat, deep] {
        processor.update(target);
        for i in 0..RATE as usize / 20 {
            let x =
                (0.1 * (std::f64::consts::TAU * 997.0 * i as f64 / RATE as f64).sin()) as f32;
            let output = processor.process([x, -x]);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
            let makeup = processor.level_match_makeup_db();
            maximum_makeup_step =
                maximum_makeup_step.max((makeup - previous_makeup).abs());
            previous_makeup = makeup;
        }
    }
    assert!(
        maximum_makeup_step < 0.08,
        "limiter-active output rebind stepped Level Match telemetry by {maximum_makeup_step:.4} dB/sample"
    );
}
