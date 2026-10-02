use super::{AnalysisWorker, MusicObservation, MusicWindowBuffer};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

// No background thread: place a loss exactly between publication and retrieval.
fn published_window(observed: u64, current: u64) -> AnalysisWorker {
    let frames = vec![[0.0; 2]; 1];
    let analysis = super::measure(&frames, 48_000).unwrap();
    AnalysisWorker {
        queue: Arc::new(crossbeam_queue::ArrayQueue::new(8)),
        dropped: Arc::new(AtomicU64::new(current)),
        latest: Arc::new(Mutex::new(None)),
        visual: Arc::new(Mutex::new(None)),
        context_window: Arc::new(Mutex::new(Some(MusicObservation {
            dropped: observed,
            window: super::context::Window {
                rate: 48_000,
                frames,
                analysis,
            },
        }))),
        stop: Arc::new(AtomicBool::new(false)),
        thread: None,
    }
}

#[test]
fn loss_after_publication_is_rejected_without_waiting_for_the_analysis_thread() {
    let worker = published_window(0, 0);
    worker.dropped.fetch_add(1, Ordering::Release);
    assert!(worker.take_music_window().is_none());
    assert!(worker.context_window.lock().unwrap().is_none());
    assert!(published_window(0, 1).take_context_window().is_none());
}

#[test]
fn mailbox_counter_reset_rejects_old_audio_and_clean_silence_is_consumed_once() {
    assert!(published_window(7, 0).take_music_window().is_none());
    let worker = published_window(7, 7);
    let window = worker.take_music_window().unwrap();
    assert_eq!(window.frames, vec![[0.0; 2]; 1]);
    assert_eq!(window.analysis.frames, 1);
    assert!(worker.take_music_window().is_none());
}

#[test]
fn recognition_waits_for_three_real_seconds_at_every_supported_rate() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let mut buffer = MusicWindowBuffer::new(rate);
        let count = rate as usize * 3;
        for index in 0..count {
            let frame = [index as f32 / count as f32, -0.05];
            assert_eq!(buffer.push(frame), index + 1 == count);
        }
        assert_eq!(buffer.frames.len(), count);
        assert_eq!(buffer.frames[0], [0.0, -0.05]);
        assert_eq!(
            buffer.frames[count - 1],
            [(count - 1) as f32 / count as f32, -0.05]
        );
    }
}

#[test]
fn dropped_audio_discards_partial_recognition_and_the_pre_gap_queue() {
    let rate = 48_000;
    let mut buffer = MusicWindowBuffer::new(rate);
    for _ in 0..rate * 2 {
        assert!(!buffer.push([0.8; 2]));
    }
    assert!(buffer.observe_loss(1, 17));
    assert!(buffer.frames.is_empty());
    for _ in 0..17 {
        assert!(!buffer.push([0.8; 2]));
    }
    assert!(buffer.frames.is_empty());
    assert!(!buffer.observe_loss(1, 0));
    for index in 0..rate * 3 {
        assert_eq!(buffer.push([0.02; 2]), index + 1 == rate * 3);
    }
    assert!(buffer.frames.iter().all(|frame| *frame == [0.02; 2]));
}

#[test]
fn another_loss_restarts_the_skip_without_extending_a_partial_window() {
    let mut buffer = MusicWindowBuffer::new(48_000);
    assert!(buffer.observe_loss(1, 4));
    assert!(!buffer.push([0.7; 2]));
    assert!(!buffer.push([0.7; 2]));
    assert!(buffer.observe_loss(2, 3));
    for _ in 0..3 {
        assert!(!buffer.push([0.7; 2]));
    }
    assert!(buffer.frames.is_empty());
    assert!(!buffer.push([0.01; 2]));
    assert_eq!(buffer.frames, vec![[0.01; 2]]);
    // A counter reset is also a discontinuity, not permission to retain old PCM.
    assert!(buffer.observe_loss(0, 0));
    assert!(buffer.frames.is_empty());
}
