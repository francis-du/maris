use super::*;
use crate::{music::MusicProfile, profile::Profile};

#[test]
fn high_makeup_recovers_smoothly_after_discontinuity() {
    const RATE: u32 = 48_000;
    let music = MusicProfile {
        highpass_hz: Some(200.0),
        adaptive: crate::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let settings = Settings::compile(&Profile::default(), RATE)
        .unwrap()
        .with_music(&music, RATE)
        .unwrap();
    let mut render = RenderState::new(settings, RATE);

    let sample = |index: usize| {
        (0.02 * (std::f64::consts::TAU * 80.0 * index as f64 / f64::from(RATE)).sin()) as f32
    };
    let mut phase = 0usize;
    let mut baseline_power = 0.0_f64;
    for i in 0..(RATE as usize * 5) {
        let x = sample(phase);
        phase += 1;
        let y = render.frame([x, x], true, false);
        if i >= RATE as usize * 4 {
            baseline_power += f64::from(y[0]).powi(2);
        }
    }
    baseline_power /= f64::from(RATE);
    let makeup = render.processor.level_match_makeup_db();
    assert!(
        makeup > 3.0,
        "fixture did not establish meaningful residual makeup: {makeup:.3} dB"
    );

    for _ in 0..(RATE / 20) {
        let y = render.frame([0.0, 0.0], false, false);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 0.891252));
    }
    assert!(
        render.previous.iter().all(|v| v.abs() < 1e-6),
        "discontinuity concealment did not fade fully to silence: {:?}",
        render.previous
    );

    let mut previous = [0.0_f32; 2];
    let mut max_step = 0.0_f32;
    for _ in 0..(RATE / 40) {
        let x = sample(phase);
        phase += 1;
        let y = render.frame([x, x], true, false);
        for channel in 0..2 {
            max_step = max_step.max((y[channel] - previous[channel]).abs());
            assert!(y[channel].is_finite() && y[channel].abs() <= 0.891252);
        }
        previous = y;
    }
    assert!(
        max_step < 0.01,
        "high-makeup recovery produced a click-sized sample step: {max_step}"
    );

    let mut recovered_power = 0.0_f64;
    for i in 0..(RATE as usize * 5) {
        let x = sample(phase);
        phase += 1;
        let y = render.frame([x, x], true, false);
        if i >= RATE as usize * 4 {
            recovered_power += f64::from(y[0]).powi(2);
        }
    }
    recovered_power /= f64::from(RATE);
    let delta_db = 10.0 * (recovered_power / baseline_power).log10();
    assert!(
        delta_db.abs() < 0.5,
        "Level Match did not reconverge after discontinuity: {delta_db:.3} dB"
    );
}
