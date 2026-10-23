//! Real RNNoise worker and its production source adapter; generated PCM, no devices.
use super::*;
use std::time::{Duration, Instant};

fn wait_until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "worker did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn tone(index: usize) -> [f32; 2] {
    let x = (std::f32::consts::TAU * 997.0 * index as f32 / 48_000.0).sin() * 0.3;
    [x, x * 0.6]
}

fn recovery_after(event: usize) {
    let input = Arc::new(ArrayQueue::new(28 * 480));
    for i in 0..input.capacity() {
        input.push(tone(i)).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    let (worker, mut source) =
        prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    wait_until(|| worker.output.is_full());
    // The worker is stopped by its full output, so all remaining raw data is old.
    while input.push(tone(1000)).is_ok() {}
    assert!(input.is_full());
    match event {
        0 => {
            metrics.overruns.fetch_add(1, Ordering::Release);
        }
        1 => {
            metrics
                .capture_discontinuities
                .fetch_add(1, Ordering::Release);
        }
        2 => {
            metrics
                .callback_discontinuities
                .fetch_add(1, Ordering::Release);
        }
        _ => {
            metrics.configuration_events.store(1, Ordering::Release);
            metrics
                .checked_configuration_events
                .store(1, Ordering::Release);
        }
    }
    for _ in 0..80 {
        let (pcm, ready) = source.frame(&metrics);
        assert!(
            !ready,
            "event {event}: old raw audio was reprocessed after downstream reset"
        );
        assert_eq!(pcm, [0.0; 2]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        input.is_empty(),
        "raw consumer did not discard the old backlog"
    );
    assert!(
        worker.output.is_empty(),
        "processed consumer did not drain stale PCM"
    );
    // After recovery, the actual model must match a new model, not keep old history.
    let mut reference = crate::models::VoiceModel::new();
    let mut expected = Vec::new();
    for block in 0..4 {
        let frames = std::array::from_fn(|i| if block < 2 { [0.0; 2] } else { tone(4000 + i) });
        let mut processed = [[0.0; 2]; 480];
        reference.process(&frames, &mut processed);
        expected.extend_from_slice(&processed);
        for frame in frames {
            input.push(frame).unwrap();
        }
    }
    // The source is not polled here; this test is now the sole output consumer.
    wait_until(|| worker.output.len() == expected.len());
    let actual: Vec<_> = (0..expected.len())
        .map(|_| worker.output.pop().unwrap())
        .collect();
    assert_eq!(
        actual, expected,
        "pre-event RNNoise history survived recovery"
    );
    assert!(worker.healthy());
}

#[test]
fn capture_overflow_cannot_reintroduce_old_raw_audio() {
    recovery_after(0);
}
#[test]
fn capture_discontinuity_cannot_reintroduce_old_raw_audio() {
    recovery_after(1);
}
#[test]
fn callback_restart_cannot_reintroduce_old_raw_audio() {
    recovery_after(2);
}
#[test]
fn acknowledged_configuration_cannot_reintroduce_old_raw_audio() {
    recovery_after(3);
}

#[test]
fn unchecked_configuration_never_advances_the_model() {
    let input = Arc::new(ArrayQueue::new(4 * 480));
    for i in 0..input.capacity() {
        input.push(tone(i)).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    metrics.configuration_events.store(1, Ordering::Release);
    let (worker, mut source) = prepare(input, 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    for _ in 0..60 {
        assert!(!source.frame(&metrics).1);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        worker.blocks.load(Ordering::Acquire),
        0,
        "unchecked configuration was inferred"
    );
    assert!(worker.output.is_empty());
}

#[test]
fn stale_consumer_acknowledgement_cannot_release_a_newer_recovery() {
    let input = Arc::new(ArrayQueue::new(28 * 480));
    for i in 0..input.capacity() {
        input.push(tone(i)).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    let (worker, mut source) =
        prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    wait_until(|| worker.output.is_full());
    metrics.overruns.store(1, Ordering::Release);
    wait_until(|| worker.recovery.requested.load(Ordering::Acquire) == 1);
    metrics.capture_discontinuities.store(1, Ordering::Release);
    wait_until(|| worker.recovery.requested.load(Ordering::Acquire) == 2);
    worker.recovery.drained.store(1, Ordering::Release);
    std::thread::sleep(Duration::from_millis(20));
    assert_ne!(worker.recovery.settled.load(Ordering::Acquire), 2);
    assert_eq!(worker.blocks.load(Ordering::Acquire), 25);
    wait_until(|| {
        assert!(!source.frame(&metrics).1);
        worker.recovery.ready()
    });
    assert_eq!(worker.recovery.settled.load(Ordering::Acquire), 2);
    assert!(input.is_empty() && worker.output.is_empty());
}

#[test]
fn pending_recovery_can_stop_without_a_running_output_consumer() {
    let input = Arc::new(ArrayQueue::new(480));
    let metrics = Arc::new(Metrics::default());
    let (worker, _source) = prepare(input, 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    metrics.callback_discontinuities.store(1, Ordering::Release);
    wait_until(|| worker.recovery.requested.load(Ordering::Acquire) == 1);
    let alive = worker.alive.clone();
    drop(worker);
    assert!(!alive.load(Ordering::Acquire));
}

#[test]
fn actual_renderer_recovers_to_fresh_digital_silence_after_configuration_change() {
    use crate::{audio::bridge::Renderer, dsp::Settings, profile::Profile};
    let input = Arc::new(ArrayQueue::new(28 * 480));
    for i in 0..input.capacity() {
        input.push(tone(i)).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    let (worker, mut source) =
        prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    wait_until(|| worker.output.is_full());
    let mut renderer = Renderer::new(
        Settings::compile(&Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    let start = Instant::now();
    let mut data = [0.0_f32; 256];
    let mut block = 0_u32;
    let mut render = |data: &mut [f32; 256], renderer: &mut Renderer, source: &mut LiveSource| {
        renderer.render_at(
            data,
            2,
            || source.frame(&metrics),
            start + Duration::from_secs_f64(f64::from(block * 128) / 48_000.0),
        );
        block += 1;
    };
    for _ in 0..24 {
        render(&mut data, &mut renderer, &mut source);
    }
    assert!(metrics.source_started.load(Ordering::Acquire));
    metrics.configuration_events.store(1, Ordering::Release);
    for _ in 0..8 {
        render(&mut data, &mut renderer, &mut source);
        assert!(data.iter().all(|x| *x == 0.0));
    }
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    wait_until(|| {
        render(&mut data, &mut renderer, &mut source);
        assert!(
            data.iter().all(|x| *x == 0.0),
            "old model or raw history reached the renderer"
        );
        worker.recovery.ready() && input.is_empty() && worker.output.is_empty()
    });
    for _ in 0..4 * 480 {
        input.push([0.0; 2]).unwrap();
    }
    wait_until(|| worker.output.len() == 4 * 480);
    for _ in 0..12 {
        render(&mut data, &mut renderer, &mut source);
        assert!(data.iter().all(|x| *x == 0.0));
    }
    assert!(
        metrics.source_started.load(Ordering::Acquire),
        "valid digital silence failed to restart"
    );
}

#[test]
fn epoch_changed_before_worker_initialization_is_not_prevalidated() {
    let metrics = Arc::new(Metrics::default());
    let recovery = Recovery::new(metrics.clone());
    metrics.capture_discontinuities.store(1, Ordering::Release);
    assert!(!recovery.ready());
    assert_ne!(
        recovery
            .observed
            .each_ref()
            .map(|slot| slot.load(Ordering::Acquire)),
        recovery.snapshot()
    );
}

#[test]
fn consumer_drains_each_recovery_generation_only_once() {
    let recovery = Recovery::new(Arc::new(Metrics::default()));
    let queue = Arc::new(ArrayQueue::new(8192));
    let mut bridge = crate::dsp::resample::ClockBridge::new(queue.clone(), 480);
    queue.push([0.3; 2]).unwrap();
    recovery.requested.store(1, Ordering::Release);
    assert!(!recovery.prepare_source(&mut bridge));
    assert!(queue.is_empty());
    assert_eq!(recovery.drained.load(Ordering::Acquire), 1);
    // Canary detects a repeated drain after acknowledgement, while a worker
    // could concurrently observe that acknowledgement and finish settling.
    queue.push([0.0; 2]).unwrap();
    assert!(!recovery.prepare_source(&mut bridge));
    assert_eq!(queue.len(), 1, "an acknowledged drain ran again");
    recovery.settled.store(1, Ordering::Release);
    assert!(recovery.prepare_source(&mut bridge));
    assert_eq!(queue.len(), 1);
}

#[test]
fn disabled_voice_adapter_preserves_unprocessed_pcm() {
    let a = Arc::new(ArrayQueue::new(8192));
    let b = Arc::new(ArrayQueue::new(8192));
    for i in 0..4000 {
        a.push(tone(i)).unwrap();
        b.push(tone(i)).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    let (worker, mut source) = prepare(a, 48_000, 48_000, false, metrics.clone()).unwrap();
    let mut baseline = LiveSource::new(b, 48_000);
    assert!(worker.is_none());
    for _ in 0..2000 {
        assert_eq!(source.frame(&metrics), baseline.frame(&metrics));
    }
}
