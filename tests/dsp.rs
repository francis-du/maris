use maris::{
    dsp::{Coefficients, Processor, Settings},
    profile::{Band, Profile, PRESETS},
};
use std::f64::consts::PI;

#[test]
fn peaking_filter_has_requested_center_gain() {
    for rate in [44100, 48000, 96000] {
        for gain in [-12.0, -3.0, 0.0, 6.0, 12.0] {
            let coefficients = Coefficients::peaking(
                Band {
                    frequency_hz: 1000.0,
                    gain_db: gain,
                    q: 1.0,
                    bandwidth_octaves: None,
                },
                rate,
            );
            assert!((coefficients.response_db(1000.0, rate) - gain).abs() < 1e-7);
        }
    }
}
#[test]
fn flat_signal_is_unity_until_actual_headroom_is_needed() {
    let profile = Profile::default();
    assert_eq!(profile.preamp_db, 0.0);
    assert_eq!(profile.effective_preamp_db(), 0.0);
    let mut processor = Processor::new(Settings::compile(&profile, 48000).unwrap());
    for index in 0..48000 {
        let x = (2.0 * PI * 1000.0 * index as f64 / 48000.0).sin() as f32 * 0.1;
        let frame = processor.process([x, -x]);
        assert!((frame[0] - x).abs() < 1e-6);
        assert!((frame[1] + x).abs() < 1e-6);
    }
}
#[test]
fn all_presets_are_finite_and_peak_limited() {
    for rate in [44100, 48000, 96000] {
        for name in PRESETS {
            let mut processor =
                Processor::new(Settings::compile(&Profile::preset(name).unwrap(), rate).unwrap());
            let mut seed = 123456789_u32;
            for _ in 0..12000 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let sample = (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32 * 2.0;
                for output in processor.process([sample, -sample * 0.8]) {
                    assert!(output.is_finite());
                    assert!(output.abs() <= 0.891252);
                }
            }
        }
    }
}
#[test]
fn nonfinite_samples_do_not_poison_the_engine() {
    let mut processor = Processor::new(Settings::compile(&Profile::default(), 48000).unwrap());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.1] {
        assert!(processor
            .process([value, value])
            .iter()
            .all(|x| x.is_finite()));
    }
    assert!(processor.process([0.1, 0.1])[0] > 0.0);
}
#[test]
fn profile_transitions_remain_finite_and_bounded() {
    let mut processor = Processor::new(Settings::compile(&Profile::default(), 48000).unwrap());
    for preset in PRESETS {
        processor.update(Settings::compile(&Profile::preset(preset).unwrap(), 48000).unwrap());
        for _ in 0..2400 {
            assert!(processor
                .process([0.5, -0.5])
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 0.891252));
        }
    }
}
#[test]
fn unsupported_sample_rates_fail() {
    assert!(Settings::compile(&Profile::default(), 8000).is_err());
    assert!(Settings::compile(&Profile::default(), 0).is_err());
}
