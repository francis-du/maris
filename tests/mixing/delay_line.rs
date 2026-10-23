//! Independent delay-line oracle and bounded storage checks; no audio devices.
use super::BusDelay;
use std::collections::VecDeque;

#[test]
fn storage_tracks_requested_delay_instead_of_always_reserving_half_a_second() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        for ms in [0.0, 0.01, 1.0, 5.0, 100.0, 500.0] {
            let delay = BusDelay::new(rate, ms).unwrap();
            let frames = (f64::from(rate) * ms / 1000.0).round() as usize;
            assert_eq!(
                delay.buffer.len(),
                frames,
                "excess storage for {ms} ms at {rate} Hz"
            );
        }
    }
}

#[test]
fn generated_validity_schedules_match_a_queue_oracle_without_changing_delayed_samples() {
    for frames in [0, 1, 2, 48, 2400] {
        for seed in 1..=8_u64 {
            let mut delay = BusDelay::new(48_000, frames as f64 / 48.0).unwrap();
            let mut history = VecDeque::from(vec![([0.0; 2], false); frames]);
            let mut state = seed;
            for index in 0..6000 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                // Long gaps, short gaps, recovery, and valid digital silence.
                let ready =
                    (100..1000).contains(&index) || (4000..4700).contains(&index) && state & 4 != 0;
                let x = if state & 8 == 0 {
                    0.0
                } else {
                    (state >> 32) as f64 / u32::MAX as f64
                };
                let input = [x, -x * 0.3];
                let (expected, output_ready) = if frames == 0 {
                    (input, ready)
                } else {
                    let output_ready = ready || history.iter().any(|(_, valid)| *valid);
                    let output = history.pop_front().unwrap().0;
                    history.push_back((input, ready));
                    (output, output_ready)
                };
                assert_eq!(
                    delay.process(input, ready),
                    (expected, output_ready),
                    "delay={frames}, seed={seed}, sample={index}"
                );
            }
        }
    }
}

#[test]
fn one_frame_delay_drains_once_even_when_the_only_valid_frame_is_silent() {
    let mut delay = BusDelay::new(48_000, 1000.0 / 48_000.0).unwrap();
    assert_eq!(delay.process([0.0; 2], false), ([0.0; 2], false));
    assert_eq!(delay.process([0.0; 2], true), ([0.0; 2], true));
    assert_eq!(delay.process([0.0; 2], false), ([0.0; 2], true));
    for _ in 0..100 {
        assert_eq!(delay.process([0.0; 2], false), ([0.0; 2], false));
    }
}

#[test]
fn invalid_delays_remain_rejected_without_clamping_the_public_limit() {
    for ms in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 500.001] {
        assert!(BusDelay::new(48_000, ms).is_err());
    }
}
