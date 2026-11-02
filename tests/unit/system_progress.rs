//! Synthetic callback clocks only: no capture permission, playback or physical devices.
use super::{CallbackProgress, Duration, Instant, Metrics, Ordering};

const TIMEOUT: Duration = Duration::from_secs(2);

fn old_single_snapshot_stall(previous: (u64, u64), current: (u64, u64)) -> bool {
    current.1 > previous.1 && current.0 == previous.0
}

#[test]
fn one_poll_gap_is_not_a_stall_on_a_slow_output() {
    let start = Instant::now();
    let previous = (10, 100);
    let current = (10, 200);
    // Ablation: the old single-snapshot rule declares this healthy gap a stall.
    assert!(old_single_snapshot_stall(previous, current));
    let mut progress = CallbackProgress::new(start, previous);
    assert!(!progress.stalled(start + Duration::from_millis(300), current, TIMEOUT));
    assert!(!progress.stalled(start + Duration::from_millis(900), (11, 300), TIMEOUT));
}

#[test]
fn loss_of_output_capture_or_both_is_detected_only_after_the_timeout() {
    for current in [(10, 101), (11, 100), (10, 100)] {
        let start = Instant::now();
        let mut progress = CallbackProgress::new(start, (10, 100));
        assert!(!progress.stalled(start + TIMEOUT - Duration::from_millis(1), current, TIMEOUT));
        assert!(progress.stalled(start + TIMEOUT, current, TIMEOUT));
    }
}

#[test]
fn the_old_detector_misses_capture_loss_and_a_fully_stopped_graph() {
    let start = Instant::now();
    for current in [(11, 100), (10, 100)] {
        assert!(!old_single_snapshot_stall((10, 100), current));
        let mut progress = CallbackProgress::new(start, (10, 100));
        assert!(progress.stalled(start + TIMEOUT, current, TIMEOUT));
    }
}

#[test]
fn live_empty_callbacks_are_healthy_without_pcm_or_amplitude_progress() {
    let start = Instant::now();
    let metrics = Metrics::default();
    let mut progress = CallbackProgress::new(start, (0, 0));
    for index in 1..100_u64 {
        metrics.callback_calls.store(index, Ordering::Release);
        metrics
            .capture_callback_calls
            .store(index, Ordering::Release);
        let current = (
            metrics.callback_calls.load(Ordering::Acquire),
            metrics.capture_callback_calls.load(Ordering::Acquire),
        );
        assert!(!progress.stalled(start + Duration::from_millis(index * 300), current, TIMEOUT));
    }
    assert_eq!(metrics.captured_frames.load(Ordering::Acquire), 0);
}

#[test]
fn progress_on_one_side_does_not_extend_the_other_sides_deadline() {
    let start = Instant::now();
    for moving_output in [false, true] {
        let mut progress = CallbackProgress::new(start, (0, 0));
        for index in 1..20_u64 {
            let current = if moving_output {
                (index, 0)
            } else {
                (0, index)
            };
            assert!(!progress.stalled(
                start + Duration::from_millis(index * 100),
                current,
                TIMEOUT
            ));
        }
        let current = if moving_output { (20, 0) } else { (0, 20) };
        assert!(progress.stalled(start + TIMEOUT, current, TIMEOUT));
    }
}

#[test]
fn counter_wrap_and_a_late_control_poll_do_not_invent_stalls() {
    let start = Instant::now();
    let mut progress = CallbackProgress::new(start, (u64::MAX, u64::MAX));
    assert!(!progress.stalled(start + Duration::from_secs(10), (0, 0), TIMEOUT));
    assert!(!progress.stalled(start + Duration::from_secs(11), (0, 0), TIMEOUT));
    assert!(progress.stalled(start + Duration::from_secs(12), (0, 0), TIMEOUT));
}

#[test]
fn slower_buffer_budget_is_honored_without_a_fixed_poll_count() {
    let start = Instant::now();
    let timeout = Duration::from_millis(4559);
    let mut progress = CallbackProgress::new(start, (0, 0));
    for index in 1..=15_u64 {
        assert!(!progress.stalled(
            start + Duration::from_millis(index * 300),
            (0, index),
            timeout
        ));
    }
    assert!(progress.stalled(start + timeout, (0, 16), timeout));
}

#[test]
fn seeded_callback_schedules_match_independent_elapsed_time_reference() {
    for seed in 1..=64_u64 {
        let start = Instant::now();
        let mut progress = CallbackProgress::new(start, (0, 0));
        let mut state = seed;
        let mut elapsed_ms = 0;
        let mut last_seen_ms = [0_u64; 2];
        let mut counters = [0_u64; 2];
        for step in 0..2000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            elapsed_ms += 1 + (state >> 32) % 40;
            for side in 0..2 {
                let disabled = step >= 1000 && (seed % 3 == side as u64 || seed % 3 == 2);
                if !disabled && state & (1 << (side + 12)) != 0 {
                    counters[side] += 1;
                    last_seen_ms[side] = elapsed_ms;
                }
            }
            let expected = last_seen_ms.iter().any(|last| elapsed_ms - last >= 2000);
            assert_eq!(
                progress.stalled(
                    start + Duration::from_millis(elapsed_ms),
                    (counters[0], counters[1]),
                    TIMEOUT
                ),
                expected,
                "seed={seed}, step={step}, counters={counters:?}"
            );
        }
    }
}
