use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn integrated_delta(rate: u32) -> f64 {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    let mut processor = Processor::new(
        Settings::compile(&profile, rate)
            .unwrap()
            .with_music(&music, rate)
            .unwrap(),
    );
    let mut input = Vec::new();
    let mut output = Vec::new();
    for i in 0..rate as usize * 4 {
        let t = i as f64 / rate as f64;
        let left = 0.06
            * (0.58 * (std::f64::consts::TAU * 83.0 * t).sin()
                + 0.27 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.15 * (std::f64::consts::TAU * 7_133.0 * t).sin());
        let right = 0.06
            * (0.51 * (std::f64::consts::TAU * 109.0 * t + 0.31).sin()
                + 0.31 * (std::f64::consts::TAU * 1_499.0 * t + 0.77).sin()
                + 0.18 * (std::f64::consts::TAU * 8_921.0 * t + 1.13).sin());
        let frame = [left as f32, right as f32];
        let processed = processor.process(frame);
        if i >= rate as usize {
            input.extend_from_slice(&frame);
            output.extend_from_slice(&processed);
        }
    }
    let mut dry = ebur128::EbuR128::new(2, rate, ebur128::Mode::I).unwrap();
    let mut wet = ebur128::EbuR128::new(2, rate, ebur128::Mode::I).unwrap();
    dry.add_frames_f32(&input).unwrap();
    wet.add_frames_f32(&output).unwrap();
    wet.loudness_global().unwrap() - dry.loudness_global().unwrap()
}

#[test]
fn r128_integrated_loudness_matching_is_sample_rate_invariant() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let delta = integrated_delta(rate);
        assert!(
            delta.abs() < 0.7,
            "{rate} Hz integrated loudness mismatch remains {delta:.3} LU"
        );
    }
}
