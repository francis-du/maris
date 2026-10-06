use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

#[test]
fn subnormal_history_cannot_poison_next_program_across_sample_rates() {
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let music = MusicProfile {
            correction_preamp_db: -30.0,
            correction_source: Some("subnormal-rate-regression".into()),
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

        for i in 0..rate as usize * 2 {
            let sign = if i.is_multiple_of(2) { 1.0 } else { -1.0 };
            let output = processor.process([f32::MIN_POSITIVE * sign, f32::from_bits(1)]);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
        }
        assert!(
            processor.level_match_makeup_db() <= 30.1,
            "{rate} Hz subnormal floor accumulated excessive makeup"
        );

        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for i in 0..rate as usize * 4 {
            let x = (0.03 * (std::f64::consts::TAU * 997.0 * i as f64 / rate as f64).sin()) as f32;
            let output = processor.process([x, -x]);
            assert!(output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
            if i >= rate as usize * 3 {
                input_power += 2.0 * f64::from(x).powi(2);
                output_power += output
                    .iter()
                    .map(|sample| f64::from(*sample).powi(2))
                    .sum::<f64>();
            }
        }
        let delta_db = 10.0 * (output_power / input_power.max(1e-30)).log10();
        assert!(
            delta_db.abs() < 1.0,
            "{rate} Hz subnormal history poisoned next-program loudness by {delta_db:.3} dB"
        );
    }
}

