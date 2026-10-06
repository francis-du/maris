use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

#[test]
fn digital_silence_releases_residual_makeup_across_sample_rates() {
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let music = MusicProfile {
            highpass_hz: Some(200.0),
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        let mut processor = Processor::new(
            Settings::compile(&Profile::default(), rate)
                .unwrap()
                .with_music(&music, rate)
                .unwrap(),
        );

        for i in 0..rate as usize * 4 {
            let x =
                (0.02 * (std::f64::consts::TAU * 80.0 * i as f64 / rate as f64).sin()) as f32;
            let _ = processor.process([x, x]);
        }
        let before = processor.level_match_makeup_db();
        assert!(
            before > 3.0,
            "{rate} Hz fixture did not establish program-dependent makeup: {before:.3} dB"
        );

        for _ in 0..rate as usize * 2 {
            let output = processor.process([0.0, 0.0]);
            assert!(output.iter().all(|sample| sample.is_finite()));
        }
        let after = processor.level_match_makeup_db();
        assert!(
            after.abs() < 0.2,
            "{rate} Hz digital silence retained stale residual makeup: {after:.3} dB"
        );

        for i in 0..rate as usize / 2 {
            let x =
                (0.08 * (std::f64::consts::TAU * 2_000.0 * i as f64 / rate as f64).sin()) as f32;
            for sample in processor.process([x, -x]) {
                assert!(sample.is_finite());
                assert!(sample.abs() <= 0.891_252);
            }
        }
    }
}