use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn settled_delta_and_limiter(profile: &Profile, music: &MusicProfile) -> (f64, f64) {
    const RATE: u32 = 48_000;
    const AMPLITUDE: f64 = 0.95;
    let mut processor = Processor::new(
        Settings::compile(profile, RATE)
            .unwrap()
            .with_music(music, RATE)
            .unwrap(),
    );
    let mut input_power = 0.0_f64;
    let mut output_power = 0.0_f64;
    let mut max_reduction = 0.0_f64;
    for i in 0..(RATE as usize * 4) {
        let t = i as f64 / RATE as f64;
        let x = AMPLITUDE
            * (0.55 * (std::f64::consts::TAU * 71.0 * t).sin()
                + 0.30 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.15 * (std::f64::consts::TAU * 6833.0 * t).sin());
        let frame = [x as f32, (-0.73 * x) as f32];
        let y = processor.process(frame);
        max_reduction = max_reduction.max(processor.limiter_reduction_db());
        for sample in y {
            assert!(sample.is_finite());
            assert!(
                sample.abs() <= 0.891_252,
                "sample peak escaped output ceiling"
            );
        }
        if i >= RATE as usize * 3 {
            input_power += x * x + (0.73 * x).powi(2);
            output_power += f64::from(y[0]).powi(2) + f64::from(y[1]).powi(2);
        }
    }
    (10.0 * (output_power / input_power).log10(), max_reduction)
}

#[test]
fn shipped_presets_keep_dense_high_level_program_near_original_loudness() {
    for eq in ["flat", "warm", "vocal", "bass", "soft", "clarity"] {
        for music_name in ["natural", "warm", "detail", "night", "long-listening"] {
            let profile = Profile::preset(eq).unwrap();
            let music = MusicProfile::preset(music_name).unwrap();
            let (delta, limiter) = settled_delta_and_limiter(&profile, &music);
            assert!(
                delta.abs() < 1.0,
                "{eq}/{music_name}: dense high-level program drifted by {delta:.3} dB with {limiter:.3} dB maximum limiter reduction"
            );
        }
    }
}

#[test]
fn deep_device_correction_reserve_still_matches_high_level_program_loudness() {
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
            correction_source: Some("high-level probe".into()),
            adaptive: maris::music::AdaptiveEq {
                enabled: false,
                strength: 0.0,
            },
            ..MusicProfile::default()
        };
        let (delta, limiter) = settled_delta_and_limiter(&profile, &music);
        assert!(
            delta.abs() < 1.0,
            "{correction_preamp_db:.0} dB correction reserve changed high-level loudness by {delta:.3} dB with {limiter:.3} dB maximum limiter reduction"
        );
    }
}
