use super::*;
use std::time::Duration;

#[test]
fn mismatched_or_lossy_observations_never_reach_the_context_backend() {
    for (reported_frames, dropped) in [(511, 0), (513, 0), (512, 1)] {
        let worker = Worker::start_from_model(Ok(None)).unwrap();
        let frames = vec![[0.01; 2]; 512];
        let mut analysis = crate::analysis::measure(&frames, 48_000).unwrap();
        analysis.frames = reported_frames;
        analysis.dropped_frames = dropped;
        worker.submit(Window {
            rate: 48_000,
            frames,
            analysis,
        });
        assert_eq!(worker.status()["dropped_windows"], 1);
        assert_eq!(worker.status()["processed_windows"], 0);
        assert!(worker.latest().is_none());
    }
}

#[test]
fn a_future_observation_cannot_block_all_later_audio() {
    let worker = Worker::start_from_model(Ok(None)).unwrap();
    let frames = vec![[0.01; 2]; 512];
    let analysis = crate::analysis::measure(&frames, 48_000).unwrap();
    let mut future = analysis.clone();
    future.updated_at_ms = u64::MAX;
    worker.submit(Window {
        rate: 48_000,
        frames: frames.clone(),
        analysis: future,
    });
    assert!(worker.state.0.lock().unwrap().last_accepted_ms.is_none());
    worker.submit(Window {
        rate: 48_000,
        frames,
        analysis: analysis.clone(),
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while worker.status()["processed_windows"] != 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        worker.latest().unwrap().updated_at_ms,
        analysis.updated_at_ms
    );
    assert_eq!(worker.status()["dropped_windows"], 1);
    assert_eq!(worker.status()["processed_windows"], 1);
}

#[test]
fn model_initialization_failure_keeps_signal_analysis_available_and_explains_why() {
    let worker = Worker::start_from_model(Err(anyhow::anyhow!("fixture model load failed")))
        .expect("optional model failure must not prevent ordinary audio startup");
    let frames = vec![[0.01; 2]; 512];
    let analysis = crate::analysis::measure(&frames, 48_000).unwrap();
    worker.submit(Window {
        rate: 48_000,
        frames,
        analysis,
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while worker.latest().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let latest = worker.latest().expect("signal fallback did not run");
    assert_eq!(latest.mode, "signal_only");
    assert!(latest.genre.is_empty());
    assert_eq!(worker.status()["semantic_backend_available"], false);
    assert!(worker.status()["last_error"]
        .as_str()
        .unwrap()
        .contains("fixture model load failed"));
    worker.reset();
    assert!(worker.status()["last_error"]
        .as_str()
        .unwrap()
        .contains("fixture model load failed"));
}
