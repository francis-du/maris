use super::*;

#[test]
fn worker_cpu_percent_is_derived_from_measured_counters() {
    let metrics = Metrics::default();
    metrics.worker_frames.store(480, Ordering::Relaxed);
    metrics
        .worker_processing_nanos
        .store(5_000_000, Ordering::Relaxed);
    let status = performance_status(&metrics, 48_000, 0, 0, None);
    assert_eq!(status["worker_cpu_percent"], json!(50.0));
}

#[test]
fn worker_cpu_percent_is_null_before_frames() {
    let metrics = Metrics::default();
    let status = performance_status(&metrics, 48_000, 0, 0, None);
    assert_eq!(status["worker_cpu_percent"], Value::Null);
}

#[test]
fn mixer_status_reports_the_shared_performance_fields() {
    let metrics = Metrics::default();
    metrics.callback_frames.store(480, Ordering::Relaxed);
    metrics
        .callback_processing_nanos
        .store(1_000_000, Ordering::Relaxed);
    metrics.worker_frames.store(480, Ordering::Relaxed);
    metrics
        .worker_processing_nanos
        .store(2_000_000, Ordering::Relaxed);
    let status = performance_status(&metrics, 48_000, 3, 1, Some(12.5));
    assert_eq!(status["analysis_queue_depth"], json!(3));
    assert_eq!(status["update_queue_depth"], json!(1));
    assert_eq!(status["worker_frames"], json!(480));
    assert_eq!(status["worker_processing_nanos"], json!(2_000_000));
    assert_eq!(status["model_latency_ms"], json!(12.5));
    assert_eq!(status["callback_calls"], json!(0));
    assert_eq!(status["callback_frames"], json!(480));
    assert_eq!(status["worker_cpu_percent"], json!(20.0));
    assert_eq!(status["callback_processing_average_percent"], json!(10.0));
}

#[test]
fn mixer_status_keeps_a_missing_model_latency_null() {
    let metrics = Metrics::default();
    let status = performance_status(&metrics, 48_000, 0, 0, None);
    assert_eq!(status["model_latency_ms"], Value::Null);
    assert_eq!(status["analysis_queue_depth"], json!(0));
}
