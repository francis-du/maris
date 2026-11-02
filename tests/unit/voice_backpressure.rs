//! Worker flow control and actual RNNoise output; generated samples, no audio devices.
use super::*;
use std::time::{Duration, Instant};

fn fill(queue: &ArrayQueue<[f32; 2]>, frames: usize) {
    for _ in 0..frames {
        queue.push([0.0; 2]).unwrap();
    }
}

#[test]
fn full_output_keeps_the_next_input_block_unconsumed() {
    let input = ArrayQueue::new(960);
    let output = ArrayQueue::new(960);
    fill(&input, 480);
    fill(&output, 960);
    assert!(
        !block_ready(&input, &output),
        "full processed queue still permits model work and silent frame loss"
    );
    assert_eq!(input.len(), 480);
}

#[test]
fn output_reserve_boundary_requires_one_complete_model_block() {
    let input = ArrayQueue::new(960);
    fill(&input, 480);
    for available in [0, 1, 479, 480, 481, 960] {
        let output = ArrayQueue::new(960);
        fill(&output, 960 - available);
        assert_eq!(
            block_ready(&input, &output),
            available >= 480,
            "available={available}"
        );
    }
}

#[test]
fn partial_input_is_never_padded_into_a_live_inference_block() {
    let input = ArrayQueue::new(960);
    let output = ArrayQueue::new(960);
    for frames in 0..480 {
        assert!(
            !block_ready(&input, &output),
            "partial block of {frames} frames accepted"
        );
        input.push([0.0; 2]).unwrap();
    }
    assert!(block_ready(&input, &output));
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "worker did not reach the expected state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn actual_worker_preserves_model_samples_across_full_and_partial_output_reserves() {
    let input = Arc::new(ArrayQueue::new(28 * 480));
    let worker = VoiceWorker::start(input.clone(), 48_000, Arc::new(Metrics::default())).unwrap();
    let mut reference = crate::models::VoiceModel::new();
    let mut expected = Vec::with_capacity(28 * 480);
    for block in 0..28 {
        let frames = std::array::from_fn(|i| {
            let t = (block * 480 + i) as f32 / 48_000.0;
            let x = (std::f32::consts::TAU * 997.0 * t).sin() * 0.2;
            [x, x * 0.6]
        });
        let mut result = [[0.0; 2]; 480];
        reference.process(&frames, &mut result);
        expected.extend_from_slice(&result);
        for frame in frames {
            input.push(frame).unwrap();
        }
    }
    wait_until(|| worker.output.is_full());
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        worker.blocks.load(Ordering::Acquire),
        25,
        "model advanced into a full output queue"
    );
    assert_eq!(input.len(), 3 * 480, "backpressure lost captured input");
    let mut actual = Vec::with_capacity(expected.len());
    for _ in 0..479 {
        actual.push(worker.output.pop().unwrap());
    }
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        worker.blocks.load(Ordering::Acquire),
        25,
        "partial output reserve split a model block"
    );
    wait_until(|| {
        while let Some(frame) = worker.output.pop() {
            actual.push(frame);
        }
        actual.len() == expected.len()
    });
    assert_eq!(
        actual, expected,
        "backpressure changed the uninterrupted RNNoise sample sequence"
    );
    assert_eq!(worker.drops.load(Ordering::Acquire), 0);
    assert!(worker.healthy());
}

#[test]
fn stopping_a_backpressured_worker_joins_without_needing_an_output_consumer() {
    let input = Arc::new(ArrayQueue::new(960));
    let worker = VoiceWorker::start(input.clone(), 48_000, Arc::new(Metrics::default())).unwrap();
    // Input is empty, so this test is the only producer until the queue is full.
    fill(&worker.output, worker.output.capacity());
    fill(&input, 960);
    let alive = worker.alive.clone();
    drop(worker);
    assert!(
        !alive.load(Ordering::Acquire),
        "Drop returned before the owned thread exited"
    );
}
