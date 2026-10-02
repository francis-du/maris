use maris::analysis::AnalysisWorker;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn feed(worker: &AnalysisWorker, value: f32) {
    feed_frames(worker, value, 96_000);
}

fn feed_frames(worker: &AnalysisWorker, value: f32, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0..count {
        while worker.queue.push([value; 2]).is_err() {
            assert!(
                Instant::now() < deadline,
                "analysis consumer did not progress"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn wait_for_rms(worker: &AnalysisWorker, expected: f64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if worker
            .latest()
            .is_some_and(|analysis| (analysis.rms_dbfs - expected).abs() < 0.001)
        {
            break;
        }
        assert!(Instant::now() < deadline, "analysis was not published");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn two_second_levels_do_not_become_a_padded_music_window() {
    let worker = AnalysisWorker::start(48_000).unwrap();
    feed(&worker, 0.1);
    wait_for_rms(&worker, -20.0);
    assert_eq!(worker.latest().unwrap().frames, 96_000);
    assert!(worker.take_music_window().is_none());
    feed_frames(&worker, 0.01, 48_000);
    let deadline = Instant::now() + Duration::from_secs(5);
    let window = loop {
        if let Some(window) = worker.take_music_window() {
            break window;
        }
        assert!(
            Instant::now() < deadline,
            "complete music window was not published"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(window.frames.len(), 144_000);
    assert_eq!(window.analysis.frames, 144_000);
    assert!(window.frames[..96_000]
        .iter()
        .all(|frame| *frame == [0.1; 2]));
    assert!(window.frames[96_000..]
        .iter()
        .all(|frame| *frame == [0.01; 2]));
    let expected_rms = 10.0 * ((2.0 * 0.1_f64.powi(2) + 0.01_f64.powi(2)) / 3.0).log10();
    assert!((window.analysis.rms_dbfs - expected_rms).abs() < 0.001);
    // Music recognition must not delay or replace the two-second level meter.
    assert!((worker.latest().unwrap().rms_dbfs + 20.0).abs() < 0.001);
}

#[test]
fn context_samples_and_statistics_are_atomic_and_latest_wins() {
    let worker = AnalysisWorker::start(48_000).unwrap();
    feed_frames(&worker, 0.1, 144_000);
    wait_for_rms(&worker, -20.0);
    feed_frames(&worker, 0.01, 144_000);
    wait_for_rms(&worker, -40.0);
    // The control consumer deliberately did not take the first window.
    let deadline = Instant::now() + Duration::from_secs(5);
    let window = loop {
        if let Some(window) = worker.take_music_window() {
            if (window.analysis.rms_dbfs + 40.0).abs() < 0.001 {
                break window;
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(window.rate, 48_000);
    assert_eq!(window.frames.len(), 144_000);
    assert_eq!(window.frames.len() as u64, window.analysis.frames);
    assert!(window.frames.iter().flatten().all(|sample| *sample == 0.01));
    assert_eq!(window.analysis.dropped_frames, 0);
}

#[test]
fn lossy_analysis_does_not_leave_an_old_window_available_for_semantic_inference() {
    let worker = AnalysisWorker::start(48_000).unwrap();
    feed(&worker, 0.1);
    wait_for_rms(&worker, -20.0);
    feed(&worker, 0.2);
    // A different second measurement proves the first three-second recognition
    // window was published, without consuming that window from its mailbox.
    wait_for_rms(&worker, 20.0 * 0.2_f64.log10());
    worker.dropped.fetch_add(10, Ordering::Relaxed);
    feed(&worker, 0.01);
    wait_for_rms(&worker, -40.0);
    assert!(worker.take_music_window().is_none());
    assert_eq!(worker.latest().unwrap().dropped_frames, 10);
}
