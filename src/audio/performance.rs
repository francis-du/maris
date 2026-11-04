//! Real-time-budget percentages derived from accumulated processing counters.

use serde_json::{json, Value};
use std::sync::atomic::Ordering;

use super::Metrics;

/// Processing seconds per real second, as a percentage.
///
/// Returns `None` when no frames have been observed or the share is not
/// finite, so callers emit JSON null instead of a fabricated or non-finite
/// number.
pub(super) fn percent(frames: u64, processing_nanos: u64, sample_rate: u32) -> Option<f64> {
    if frames == 0 {
        return None;
    }
    finite_percent(processing_nanos as f64 * sample_rate as f64 * 100.0 / (frames as f64 * 1e9))
}

fn finite_percent(percent: f64) -> Option<f64> {
    percent.is_finite().then_some(percent)
}

/// Runtime `performance` object shared by the single-session status builder.
pub(super) fn status(
    metrics: &Metrics,
    sample_rate: u32,
    analysis_queue_depth: usize,
    update_queue_depth: usize,
    model_latency_ms: Option<f64>,
) -> Value {
    let callback_frames = metrics.callback_frames.load(Ordering::Relaxed);
    let callback_average_percent = percent(
        callback_frames,
        metrics.callback_processing_nanos.load(Ordering::Relaxed),
        sample_rate,
    );
    let worker_cpu_percent = percent(
        metrics.worker_frames.load(Ordering::Relaxed),
        metrics.worker_processing_nanos.load(Ordering::Relaxed),
        sample_rate,
    );
    json!({
        "analysis_queue_depth":analysis_queue_depth,
        "update_queue_depth":update_queue_depth,
        "callback_processing_average_percent":callback_average_percent,
        "callback_processing_max_us":metrics.callback_max_nanos.load(Ordering::Relaxed) as f64 / 1000.0,
        "callback_calls":metrics.callback_calls.load(Ordering::Relaxed),
        "callback_frames":callback_frames,
        "worker_cpu_percent":worker_cpu_percent,
        "worker_processing_nanos":metrics.worker_processing_nanos.load(Ordering::Relaxed),
        "worker_frames":metrics.worker_frames.load(Ordering::Relaxed),
        "model_latency_ms":model_latency_ms,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/performance.rs"]
mod tests;
