use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn settled_delta_db(left_scale: f32, right_scale: f32) -> f64 {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    let mut processor = Processor::new(
        Settings::compile(&profile, 48_000)
            .unwrap()
            .with_music(&music, 48_000)
            .unwrap(),
    );
    let mut input = 0.0_f64;
    let mut output = 0.0_f64;
    for i in 0..384_000 {
        let t = i as f64 / 48_000.0;
        let x = (0.08 * (std::f64::consts::TAU * 220.0 * t).sin()
            + 0.05 * (std::f64::consts::TAU * 2_300.0 * t).sin()
            + 0.025 * (std::f64::consts::TAU * 8_000.0 * t).sin()) as f32;
        let frame = [x * left_scale, x * right_scale];
        let rendered = processor.process(frame);
        if i >= 288_000 {
            input += frame.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
            output += rendered.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        }
    }
    10.0 * (output / input).log10()
}

#[test]
fn asymmetric_stereo_does_not_confuse_level_match() {
    for (left, right) in [(1.0, 0.0), (1.0, 0.05), (1.0, -0.2), (0.1, 1.0)] {
        let delta = settled_delta_db(left, right);
        assert!(
            delta.abs() < 1.0,
            "{left}/{right}: settled loudness drifted by {delta:.3} dB"
        );
    }
}

#[test]
fn large_level_step_stays_peak_safe_for_asymmetric_program() {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    let mut processor = Processor::new(
        Settings::compile(&profile, 48_000)
            .unwrap()
            .with_music(&music, 48_000)
            .unwrap(),
    );
    for i in 0..240_000 {
        let amplitude = if i < 192_000 { 0.008 } else { 0.95 };
        let x = (amplitude * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin()) as f32;
        for sample in processor.process([x, x * 0.07]) {
            assert!(sample.is_finite());
            assert!(sample.abs() <= 0.891_252, "peak escaped ceiling: {sample}");
        }
    }
}