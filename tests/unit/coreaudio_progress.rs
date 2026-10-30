use super::{
    observe_progress, output_progress_timeout, output_stall_timeout, system_progress_timeout,
    ProgressStall,
};
use std::time::{Duration, Instant};

#[test]
fn progress_guard_fails_open_on_output_capture_or_combined_stalls() {
    let start = Instant::now();
    let timeout = Duration::from_millis(750);
    let (first, failure) = observe_progress(None, (10, 100), start, timeout);
    assert_eq!(failure, None);

    // No side has exceeded its window yet.
    let (early, failure) = observe_progress(
        Some(first),
        (10, 200),
        start + Duration::from_millis(600),
        timeout,
    );
    assert_eq!(failure, None);

    // Capture advances while the replacement output is stuck.
    let (_, failure) = observe_progress(
        Some(early),
        (10, 300),
        start + Duration::from_millis(800),
        timeout,
    );
    assert_eq!(failure, Some(ProgressStall::Output));

    // Output advances while the tap capture is stuck.
    let (capture_base, failure) = observe_progress(None, (20, 400), start, timeout);
    assert_eq!(failure, None);
    let (_, failure) = observe_progress(
        Some(capture_base),
        (21, 400),
        start + Duration::from_millis(800),
        timeout,
    );
    assert_eq!(failure, Some(ProgressStall::Capture));

    // If both sides stop, the physical replacement callback itself is enough evidence
    // to fail open; capture progress is not required as a witness.
    let (both_base, failure) = observe_progress(None, (30, 500), start, timeout);
    assert_eq!(failure, None);
    let (_, failure) = observe_progress(
        Some(both_base),
        (30, 500),
        start + Duration::from_millis(800),
        timeout,
    );
    assert_eq!(failure, Some(ProgressStall::OutputAndCapture));

    // Genuine progress on both sides resets both watchdog clocks.
    let (healthy, failure) = observe_progress(None, (40, 600), start, timeout);
    assert_eq!(failure, None);
    let (_, failure) = observe_progress(
        Some(healthy),
        (41, 700),
        start + Duration::from_secs(2),
        timeout,
    );
    assert_eq!(failure, None);

    // Counter discontinuity/reset is still progress, not a stall.
    let (_, failure) = observe_progress(
        Some(healthy),
        (39, 701),
        start + Duration::from_secs(2),
        timeout,
    );
    assert_eq!(failure, None);
}

#[test]
fn progress_windows_scale_with_the_real_output_buffer_period() {
    assert_eq!(
        output_stall_timeout(Some(512), 48_000),
        Duration::from_millis(750)
    );
    assert_eq!(
        output_progress_timeout(Some(512), 48_000, Duration::from_secs(1)),
        Duration::from_secs(1)
    );
    let large = output_stall_timeout(Some(65_536), 44_100);
    let startup = output_progress_timeout(Some(65_536), 44_100, Duration::from_secs(1));
    assert!(large > Duration::from_secs(4));
    assert!(large <= Duration::from_secs(5));
    assert_eq!(startup, large);
}

#[test]
fn system_watchdog_honors_the_slower_capture_or_output_budget() {
    assert_eq!(
        system_progress_timeout(Duration::from_secs(2), Some(512), 48_000),
        Duration::from_secs(2)
    );
    let slow_output = output_stall_timeout(Some(65_536), 44_100);
    assert_eq!(
        system_progress_timeout(Duration::from_secs(2), Some(65_536), 44_100),
        slow_output
    );
    assert_eq!(
        system_progress_timeout(Duration::from_secs(5), Some(65_536), 44_100),
        Duration::from_secs(5)
    );
}
