use crate::{
    dsp::{Processor, Settings},
    music::{AdaptiveEq, Compressor, MusicProfile},
    profile::Profile,
};

fn music(reference: bool, bass_db: f64, air_db: f64) -> MusicProfile {
    let mut profile = MusicProfile {
        bass_db,
        air_db,
        reference,
        level_match: true,
        adaptive: AdaptiveEq {
            enabled: true,
            strength: 0.7,
        },
        compressor: Compressor {
            enabled: true,
            threshold_db: -26.0,
            ratio: 3.0,
            attack_ms: 3.0,
            release_ms: 180.0,
            ..Compressor::default()
        },
        virtual_surround: 0.5,
        stereo_focus: 0.2,
        ..MusicProfile::default()
    };
    profile.bass_assist.enabled = true;
    profile.bass_assist.amount = 0.5;
    profile
}

fn frame(index: usize, amplitude: f64) -> [f32; 2] {
    let t = index as f64 / 48_000.0;
    [
        (amplitude
            * (0.48 * (std::f64::consts::TAU * 73.0 * t).sin()
                + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.22 * (std::f64::consts::TAU * 8_100.0 * t).sin())) as f32,
        (amplitude
            * (0.44 * (std::f64::consts::TAU * 109.0 * t + 0.3).sin()
                + 0.32 * (std::f64::consts::TAU * 1_499.0 * t + 0.7).sin()
                + 0.24 * (std::f64::consts::TAU * 7_500.0 * t + 1.1).sin())) as f32,
    ]
}

#[test]
fn rapid_reference_retunes_preserve_limiter_and_latest_target() {
    const RATE: u32 = 48_000;
    let eq = Profile::default();
    let compile = |music: &MusicProfile| {
        Settings::compile(&eq, RATE)
            .unwrap()
            .with_music(music, RATE)
            .unwrap()
            .with_transition_ms(RATE, 120)
    };
    let enhanced_a = compile(&music(false, 2.0, -1.0));
    let reference_a = compile(&music(true, 2.0, -1.0));
    let enhanced_b = compile(&music(false, -3.0, 3.0));
    let reference_b = compile(&music(true, -3.0, 3.0));
    let mut processor = Processor::new(enhanced_a);

    let mut index = 0usize;
    for _ in 0..RATE as usize * 3 {
        let _ = processor.process(frame(index, 0.08));
        index += 1;
    }

    for _ in 0..128 {
        let _ = processor.process([4.0, -4.0]);
    }
    let limiter_engaged = processor.limiter_reduction_db();
    assert!(
        limiter_engaged > 5.0,
        "fixture did not engage limiter: {limiter_engaged:.3} dB"
    );

    let mut previous_makeup = processor.level_match_makeup_db();
    let mut maximum_makeup_step = 0.0_f64;
    for settings in [reference_a, enhanced_b, reference_b, enhanced_b] {
        let limiter_before_update = processor.limiter_reduction_db();
        processor.update(settings);
        assert!(
            (processor.limiter_reduction_db() - limiter_before_update).abs() < 1e-12,
            "queueing Reference/retune released limiter attenuation"
        );
        for _ in 0..48 {
            let output = processor.process(frame(index, 0.08));
            index += 1;
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
            let makeup = processor.level_match_makeup_db();
            maximum_makeup_step = maximum_makeup_step.max((makeup - previous_makeup).abs());
            previous_makeup = makeup;
        }
    }

    assert!(
        maximum_makeup_step < 0.08,
        "rapid Reference/retune sequence stepped Level Match telemetry by {maximum_makeup_step:.4} dB/sample"
    );

    for _ in 0..RATE as usize {
        let output = processor.process(frame(index, 0.08));
        index += 1;
        assert!(output
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
    }
    assert!(
        !processor.settings_pending(),
        "latest enhanced target did not settle after rapid Reference/retune sequence"
    );

    let mut fresh = Processor::new(enhanced_b);
    for _ in 0..RATE as usize {
        let _ = fresh.process(frame(index, 0.08));
        index += 1;
    }
    let input = frame(index, 0.08);
    let actual = processor.process(input);
    let expected = fresh.process(input);
    let error = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| f64::from(*a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        error < 0.03,
        "rapid Reference/retune sequence failed to converge on latest enhanced target: max error {error:.5}"
    );
}
