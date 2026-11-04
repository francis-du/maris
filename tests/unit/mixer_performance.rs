use super::*;

#[test]
fn worker_cpu_percent_is_derived_from_measured_counters() {
    let metrics = Metrics::default();
    metrics.worker_frames.store(480, Ordering::Relaxed);
    metrics
        .worker_processing_nanos
        .store(5_000_000, Ordering::Relaxed);
    let status = performance_status(&metrics, 48_000);
    assert_eq!(status["worker_cpu_percent"], json!(50.0));
}

#[test]
fn worker_cpu_percent_is_null_before_frames() {
    let metrics = Metrics::default();
    let status = performance_status(&metrics, 48_000);
    assert_eq!(status["worker_cpu_percent"], Value::Null);
}
