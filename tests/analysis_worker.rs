use maris::analysis::AnalysisWorker;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[test]
fn native_analysis_uses_the_capture_loss_counter() {
    let dropped = Arc::new(AtomicU64::new(3));
    let worker = AnalysisWorker::start_with_dropped(48000, dropped.clone()).unwrap();
    assert!(Arc::ptr_eq(&worker.dropped, &dropped));
    let deadline = Instant::now() + Duration::from_secs(5);
    for i in 0..96000 {
        let sample = ((i as f64 * 0.13).sin() * 0.05) as f32;
        let mut frame = [sample; 2];
        loop {
            match worker.queue.push(frame) {
                Ok(()) => break,
                Err(value) => {
                    frame = value;
                    std::thread::yield_now();
                }
            }
            assert!(
                Instant::now() < deadline,
                "Analysis worker failed to consume samples"
            );
        }
    }
    loop {
        if let Some(value) = worker.latest() {
            assert_eq!(value.frames, 96000);
            assert_eq!(value.dropped_frames, dropped.load(Ordering::Relaxed));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Analysis worker failed to publish a window"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn analysis_worker_rejects_unsupported_rates_before_allocating() {
    assert!(AnalysisWorker::start(0).is_err());
    assert!(AnalysisWorker::start(u32::MAX).is_err());
}
