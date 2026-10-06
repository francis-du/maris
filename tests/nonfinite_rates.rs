use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

#[test]
fn nonfinite_frames_cannot_poison_level_match_across_sample_rates() {
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let music = MusicProfile {
            highpass_hz: Some(180.0),
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            level_match: true,
            ..MusicProfile::default()
        };
        let mut processor = Processor::new(
            Settings::compile(&Profile::default(), rate)
                .unwrap()
                .with_music(&music, rate)
                .unwrap(),
        );

        for i in 0..rate as usize * 4 {
            let x = (0.03 * (std::f64::consts::TAU * 997.0 * i as f64 / rate as f64).sin()) as f32;
            let _ = processor.process([x, -0.7 * x]);
        }
        let before = processor.level_match_makeup_db();
        assert!(before.is_finite());

        for frame in [
            [f32::NAN, f32::INFINITY],
            [f32::NEG_INFINITY, f32::NAN],
            [f32::MAX, -f32::MAX],
        ] {
            let output = processor.process(frame);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
            assert!(processor.level_match_makeup_db().is_finite());
        }

        for i in 0..rate as usize * 2 {
            let x =
                (0.03 * (std::f64::consts::TAU * 997.0 * i as f64 / rate as f64).sin()) as f32;
            let output = processor.process([x, -0.7 * x]);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
        }
        let after = processor.level_match_makeup_db();
        assert!(
            (after - before).abs() < 0.5,
            "{rate} Hz nonfinite frames poisoned Level Match history: before={before:.3} dB after={after:.3} dB"
        );
    }
}