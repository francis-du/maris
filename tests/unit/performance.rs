use super::*;

#[test]
fn percent_matches_hand_computed_real_time_budget() {
    // 480 frames at 48 kHz is 10 ms of real time; 5 ms of processing is 50%.
    assert_eq!(percent(480, 5_000_000, 48_000), Some(50.0));
    // 10 ms of processing over 10 ms of real time is 100%.
    assert_eq!(percent(480, 10_000_000, 48_000), Some(100.0));
    // One second of frames with 24 ms of processing is 2.4%.
    let observed = percent(48_000, 24_000_000, 48_000).unwrap();
    assert!((observed - 2.4).abs() < 1e-12);
}

#[test]
fn percent_is_null_before_any_frames_are_observed() {
    assert_eq!(percent(0, 0, 48_000), None);
    assert_eq!(percent(0, 1_000_000, 48_000), None);
    assert_eq!(
        serde_json::json!(percent(0, 0, 48_000)),
        serde_json::Value::Null
    );
}

#[test]
fn non_finite_share_is_reported_as_null() {
    assert_eq!(finite_percent(f64::INFINITY), None);
    assert_eq!(finite_percent(f64::NEG_INFINITY), None);
    assert_eq!(finite_percent(f64::NAN), None);
    assert_eq!(finite_percent(12.5), Some(12.5));
    assert_eq!(
        serde_json::json!(finite_percent(f64::NAN)),
        serde_json::Value::Null
    );
}

#[test]
fn extreme_counters_cannot_overflow_to_infinity() {
    let observed = percent(1, u64::MAX, u32::MAX).unwrap();
    assert!(observed.is_finite());
    assert!(observed > 0.0);
}

#[test]
fn status_derives_worker_cpu_percent_from_measured_counters() {
    let metrics = Metrics::default();
    metrics.worker_frames.store(480, Ordering::Relaxed);
    metrics
        .worker_processing_nanos
        .store(5_000_000, Ordering::Relaxed);
    let status = status(&metrics, 48_000, 0, 0, None);
    assert_eq!(status["worker_cpu_percent"], serde_json::json!(50.0));
}

#[test]
fn status_worker_cpu_percent_is_null_before_frames() {
    let metrics = Metrics::default();
    let status = status(&metrics, 48_000, 0, 0, None);
    assert_eq!(status["worker_cpu_percent"], serde_json::Value::Null);
}
