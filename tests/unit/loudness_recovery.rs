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

#[test]
fn combined_level_match_and_limiter_state_recovers_without_blast() {
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
    let sample = |index: usize, amplitude: f64| {
        (amplitude * (std::f64::consts::TAU * 80.0 * index as f64 / f64::from(RATE)).sin()) as f32
    };

    let mut phase = 0usize;
    for _ in 0..(RATE as usize * 5) {
        let x = sample(phase, 0.02);
        phase += 1;
        let _ = render.frame([x, x], true, false);
    }
    assert!(
        render.processor.level_match_makeup_db() > 3.0,
        "fixture did not establish Level Match makeup"
    );

    for i in 0..4_096 {
        let x = (4.0 * (std::f64::consts::TAU * 1_000.0 * i as f64 / f64::from(RATE)).sin()) as f32;
        let _ = render.frame([x, -x], true, false);
    }
    let limiter_before = render.processor.limiter_reduction_db();
    assert!(
        limiter_before > 5.0,
        "fixture did not establish limiter attenuation: {limiter_before:.3} dB"
    );

    for _ in 0..(RATE / 20) {
        let y = render.frame([0.0, 0.0], false, false);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 0.891252));
    }

    let x = sample(phase, 0.02);
    phase += 1;
    let first = render.frame([x, x], true, false);
    assert!(
        render.processor.level_match_makeup_db() < 0.5,
        "discontinuity retained stale residual makeup"
    );
    let limiter_after = render.processor.limiter_reduction_db();
    assert!(
        limiter_after > 1.0 && limiter_after <= limiter_before + 1e-9,
        "discontinuity released limiter too aggressively: before={limiter_before:.3} after={limiter_after:.3} dB"
    );
    assert!(
        first.iter().all(|v| v.is_finite() && v.abs() <= 0.891252),
        "first recovery frame exceeded output ceiling: {first:?}"
    );

    for _ in 0..(RATE as usize * 5) {
        let x = sample(phase, 0.02);
        phase += 1;
        let y = render.frame([x, x], true, false);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 0.891252));
    }
    assert!(
        render.processor.level_match_makeup_db() > 3.0,
        "Level Match failed to reconverge after combined-state recovery"
    );
    assert!(
        render.processor.limiter_reduction_db() < 0.2,
        "limiter failed to release after recovery: {:.3} dB",
        render.processor.limiter_reduction_db()
    );
}

#[test]
fn discontinuity_during_effect_retune_preserves_target_limiter_and_safe_recovery() {
    const RATE: u32 = 48_000;
    let base_music = MusicProfile {
        highpass_hz: Some(200.0),
        adaptive: crate::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: true,
        ..MusicProfile::default()
    };
    let mut target_music = base_music.clone();
    target_music.adaptive.enabled = true;
    target_music.adaptive.strength = 1.0;
    target_music.compressor.enabled = true;
    target_music.compressor.threshold_db = -30.0;
    target_music.compressor.ratio = 4.0;
    target_music.bass_assist.enabled = true;
    target_music.bass_assist.amount = 0.8;

    let base = Settings::compile(&Profile::default(), RATE)
        .unwrap()
        .with_music(&base_music, RATE)
        .unwrap();
    let target = Settings::compile(&Profile::default(), RATE)
        .unwrap()
        .with_music(&target_music, RATE)
        .unwrap()
        .with_transition_ms(RATE, 120);
    let mut render = RenderState::new(base, RATE);
    let mut reference = Processor::new(base);
    reference.update(target);

    for i in 0..(RATE as usize * 4) {
        let x = (0.02 * (std::f64::consts::TAU * 80.0 * i as f64 / f64::from(RATE)).sin()) as f32;
        let _ = render.frame([x, x], true, false);
    }
    for i in 0..4_096 {
        let x = (4.0 * (std::f64::consts::TAU * 1_000.0 * i as f64 / f64::from(RATE)).sin()) as f32;
        let _ = render.frame([x, -x], true, false);
    }
    let limiter_before = render.processor.limiter_reduction_db();
    assert!(limiter_before > 5.0);

    render.processor.update(target);
    for i in 0..(RATE as usize / 500) {
        let x = (0.08 * (std::f64::consts::TAU * 997.0 * i as f64 / f64::from(RATE)).sin()) as f32;
        let input = [x, -0.7 * x];
        let _ = render.frame(input, true, false);
        let _ = reference.process(input);
    }
    assert!(render.processor.settings_pending());

    for _ in 0..(RATE / 20) {
        let y = render.frame([0.0, 0.0], false, false);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 0.891252));
    }
    assert!(render.processor.settings_pending());
    let limiter_after_gap = render.processor.limiter_reduction_db();
    assert!(
        limiter_after_gap > 1.0 && limiter_after_gap <= limiter_before + 1e-9,
        "retune discontinuity released limiter: before={limiter_before:.3} after={limiter_after_gap:.3} dB"
    );
    reference.reset_history();

    let mut previous = [0.0_f32; 2];
    let mut max_step = 0.0_f32;
    for i in 0..(RATE as usize / 4) {
        let x = (0.06 * (std::f64::consts::TAU * 997.0 * i as f64 / f64::from(RATE)).sin()) as f32;
        let input = [x, -0.7 * x];
        let y = render.frame(input, true, false);
        let _ = reference.process(input);
        if i == 0 {
            let actual_makeup = render.processor.level_match_makeup_db();
            let reference_makeup = reference.level_match_makeup_db();
            assert!(
                (actual_makeup - reference_makeup).abs() < 1e-9,
                "recovery replayed stale Level Match history: actual={actual_makeup:.6} reference={reference_makeup:.6} dB"
            );
        }
        for channel in 0..2 {
            max_step = max_step.max((y[channel] - previous[channel]).abs());
            assert!(y[channel].is_finite() && y[channel].abs() <= 0.891252);
        }
        previous = y;
    }
    assert!(
        max_step < 0.08,
        "recovery during retune produced a click-sized sample step: {max_step}"
    );
    assert!(
        !render.processor.settings_pending(),
        "effect retune target was lost or stalled across discontinuity"
    );
}

#[test]
fn pending_retune_recovery_obeys_output_envelope_slope() {
    const RATE: u32 = 48_000;
    let base = Settings::compile(&Profile::default(), RATE).unwrap();
    let target = Settings::compile(
        &Profile {
            stereo_width: 0.5,
            ..Profile::default()
        },
        RATE,
    )
    .unwrap()
    .with_transition_ms(RATE, 120);
    let mut render = RenderState::new(base, RATE);

    for _ in 0..(RATE / 20) {
        let _ = render.frame([0.5, -0.25], true, false);
    }
    render.processor.update(target);
    for _ in 0..(RATE / 500) {
        let _ = render.frame([0.5, -0.25], true, false);
    }
    assert!(render.processor.settings_pending());

    let mut previous = render.previous;
    let max_step = 2.0 / (RATE as f32 * 0.025);
    for _ in 0..16 {
        let y = render.frame([0.0, 0.0], false, false);
        for channel in 0..2 {
            assert!(
                (y[channel] - previous[channel]).abs() <= max_step,
                "gap concealment exceeded output envelope slope"
            );
        }
        previous = y;
    }

    for _ in 0..(RATE / 20) {
        let y = render.frame([-0.5, 0.25], true, false);
        for channel in 0..2 {
            assert!(
                (y[channel] - previous[channel]).abs() <= max_step,
                "recovery with pending retune exceeded output envelope slope: previous={previous:?} current={y:?}"
            );
            assert!(y[channel].is_finite() && y[channel].abs() <= 0.891252);
        }
        previous = y;
    }
    assert!(
        render.processor.settings_pending(),
        "120 ms retune finished before the 50 ms recovery window elapsed"
    );
    for _ in 0..(RATE / 10) {
        let y = render.frame([-0.5, 0.25], true, false);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 0.891252));
    }
    assert!(
        !render.processor.settings_pending(),
        "pending retune did not finish after sufficient recovery audio"
    );
}

