//! Deterministic snapshot/handshake interleavings on the production source path.
//! Generated PCM and real RNNoise workers only; no native capture or playback.
use super::*;
use crate::audio::voice;

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "worker did not reach the scheduled handoff"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn push_fresh(input: &ArrayQueue<[f32; 2]>) {
    for index in 0..1920 {
        let sample = (std::f32::consts::TAU * 997.0 * index as f32 / 48_000.0).sin() * 0.2;
        input.push([sample, sample * 0.6]).unwrap();
    }
}

fn assert_fresh_sequence(source: &mut LiveSource, metrics: &Metrics) {
    assert_fresh_sequence_at_rate(source, metrics, 48_000);
}

fn assert_fresh_sequence_at_rate(source: &mut LiveSource, metrics: &Metrics, output_rate: u32) {
    let mut model = crate::models::VoiceModel::new();
    let raw = ArrayQueue::new(1920);
    push_fresh(&raw);
    let expected = Arc::new(ArrayQueue::new(4096));
    for _ in 0..4 {
        let block = std::array::from_fn(|_| raw.pop().unwrap());
        let mut processed = [[0.0; 2]; 480];
        model.process(&block, &mut processed);
        for frame in processed {
            expected.push(frame).unwrap();
        }
    }
    let mut baseline = LiveSource::with_rates(expected, 48_000, output_rate).unwrap();
    let baseline_metrics = Metrics::default();
    let mut nonzero = false;
    // Observe the same input duration at every output rate, beyond RNNoise's
    // first silent block; a fixed output-frame count would miss it at 192 kHz.
    let output_frames = (1024_u64 * u64::from(output_rate)).div_ceil(48_000);
    for frame in 0..output_frames {
        let expected = baseline.frame(&baseline_metrics);
        assert!(expected.1, "reference must have real processed PCM");
        nonzero |= expected.0.iter().any(|sample| *sample != 0.0);
        assert_eq!(
            source.frame(metrics),
            expected,
            "fresh processed sample {frame} was lost or changed"
        );
    }
    assert!(
        nonzero,
        "the comparison must include more than the model's initial silent block"
    );
}

fn notify(metrics: &Metrics, event: usize) {
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
}

#[test]
fn delayed_snapshot_after_voice_ack_does_not_discard_the_first_fresh_block() {
    for event in 0..4 {
        let metrics = Arc::new(Metrics::default());
        let input = Arc::new(ArrayQueue::new(4096));
        let (worker, mut source) =
            voice::prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
        let worker = worker.unwrap();
        let recovery = source.voice_recovery.as_ref().unwrap().clone();
        // The callback has already sampled the counters, then is descheduled.
        let sampled_before_notification = (0, 0, 0, 0);
        notify(&metrics, event);
        wait_until(|| worker.status()["recovery_generation"].as_u64() == Some(1));
        // Resume precisely between frame()'s snapshot and its ordinary consumer
        // path. No fake producer or manually written handshake state is used.
        assert!(
            !source
                .frame_observed(&metrics, sampled_before_notification)
                .1
        );
        wait_until(|| recovery.ready());
        assert_eq!(
            metrics.stream_resets.load(Ordering::Acquire),
            1,
            "count the acknowledged drain before later snapshots"
        );
        push_fresh(&input);
        wait_until(|| worker.output.len() == 1920);
        assert_fresh_sequence(&mut source, &metrics);
        assert!(
            !worker.output.is_empty(),
            "event {event}: a second consumer reset drained fresh audio"
        );
        assert_eq!(worker.status()["recovery_generation"], 1);
        assert_eq!(worker.status()["dropped_frames"], 0);
    }
}

#[test]
fn preexisting_valid_capture_counters_do_not_discard_new_voice_output() {
    let metrics = Arc::new(Metrics::default());
    metrics.overruns.store(7, Ordering::Release);
    metrics.capture_discontinuities.store(11, Ordering::Release);
    metrics.configuration_events.store(13, Ordering::Release);
    metrics
        .checked_configuration_events
        .store(13, Ordering::Release);
    let input = Arc::new(ArrayQueue::new(4096));
    let (worker, mut source) =
        voice::prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    push_fresh(&input);
    wait_until(|| worker.output.len() == 1920);
    assert_fresh_sequence(&mut source, &metrics);
    assert!(!worker.output.is_empty());
    assert_eq!(worker.status()["recovery_generation"], 0);
    assert_eq!(
        metrics.stream_resets.load(Ordering::Acquire),
        0,
        "a valid initial snapshot did not reset the stream"
    );
}

#[test]
fn consecutive_recoveries_each_preserve_their_own_fresh_sequence() {
    let metrics = Arc::new(Metrics::default());
    let input = Arc::new(ArrayQueue::new(4096));
    let (worker, mut source) =
        voice::prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    let recovery = source.voice_recovery.as_ref().unwrap().clone();
    for generation in 1..=8 {
        let sampled = (generation - 1, 0, 0, 0);
        metrics.overruns.store(generation, Ordering::Release);
        wait_until(|| worker.status()["recovery_generation"].as_u64() == Some(generation));
        assert!(!source.frame_observed(&metrics, sampled).1);
        wait_until(|| recovery.ready());
        push_fresh(&input);
        wait_until(|| worker.output.len() == 1920);
        assert_fresh_sequence(&mut source, &metrics);
    }
    assert_eq!(worker.status()["dropped_frames"], 0);
}

#[test]
fn a_pending_configuration_remains_blocked_until_its_native_acknowledgement() {
    let metrics = Arc::new(Metrics::default());
    let input = Arc::new(ArrayQueue::new(4096));
    let (worker, mut source) =
        voice::prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    let recovery = source.voice_recovery.as_ref().unwrap().clone();
    push_fresh(&input);
    wait_until(|| worker.output.len() == 1920);
    metrics.configuration_events.store(1, Ordering::Release);
    wait_until(|| worker.status()["recovery_generation"] == 1);
    for _ in 0..8 {
        assert_eq!(source.frame(&metrics), ([0.0; 2], false));
        assert!(!recovery.ready());
    }
    assert!(worker.output.is_empty());
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    wait_until(|| recovery.ready());
    push_fresh(&input);
    wait_until(|| worker.output.len() == 1920);
    assert_fresh_sequence(&mut source, &metrics);
}

#[test]
fn delayed_voice_snapshot_preserves_pcm_across_rates_and_counter_wrap() {
    for output_rate in [44_100, 48_000, 96_000, 192_000] {
        for previous in [7_u64, u64::MAX] {
            let metrics = Arc::new(Metrics::default());
            metrics.overruns.store(previous, Ordering::Release);
            let input = Arc::new(ArrayQueue::new(4096));
            let (worker, mut source) =
                voice::prepare(input.clone(), 48_000, output_rate, true, metrics.clone()).unwrap();
            let worker = worker.unwrap();
            let recovery = source.voice_recovery.as_ref().unwrap().clone();
            metrics
                .overruns
                .store(previous.wrapping_add(1), Ordering::Release);
            wait_until(|| worker.status()["recovery_generation"] == 1);
            assert!(!source.frame_observed(&metrics, (previous, 0, 0, 0)).1);
            wait_until(|| recovery.ready());
            push_fresh(&input);
            wait_until(|| worker.output.len() == 1920);
            assert_fresh_sequence_at_rate(&mut source, &metrics, output_rate);
            assert_eq!(metrics.stream_resets.load(Ordering::Acquire), 1);
            assert_eq!(worker.status()["dropped_frames"], 0);
        }
    }
}

#[test]
fn drain_ownership_ablation_reproduces_fresh_pcm_loss_after_a_settled_handoff() {
    for keep_voice_owner in [false, true] {
        let metrics = Arc::new(Metrics::default());
        let input = Arc::new(ArrayQueue::new(4096));
        let (worker, mut source) =
            voice::prepare(input.clone(), 48_000, 48_000, true, metrics.clone()).unwrap();
        let worker = worker.unwrap();
        let recovery = source.voice_recovery.as_ref().unwrap().clone();
        metrics.overruns.store(1, Ordering::Release);
        wait_until(|| worker.status()["recovery_generation"] == 1);
        assert!(!source.frame_observed(&metrics, (0, 0, 0, 0)).1);
        wait_until(|| recovery.ready());
        push_fresh(&input);
        wait_until(|| worker.output.len() == 1920);
        assert!(input.is_empty() && recovery.ready());
        if keep_voice_owner {
            assert_fresh_sequence(&mut source, &metrics);
            assert!(!worker.output.is_empty());
            assert_eq!(metrics.stream_resets.load(Ordering::Acquire), 1);
        } else {
            // Test-only ablation after a fully settled handshake: all readiness
            // checks are true, so returning drain ownership to the ordinary
            // observer isolates its delayed-reset effect. Never a runtime option.
            source.voice_recovery = None;
            assert_eq!(source.frame(&metrics), ([0.0; 2], false));
            assert!(
                worker.output.is_empty(),
                "ablation did not remove fresh PCM"
            );
            assert_eq!(metrics.stream_resets.load(Ordering::Acquire), 2);
        }
    }
}

#[test]
fn ordinary_unprocessed_source_still_discards_old_pcm_on_each_notification() {
    for event in 0..4 {
        let metrics = Arc::new(Metrics::default());
        let queue = Arc::new(ArrayQueue::new(4096));
        push_fresh(&queue);
        let mut source = LiveSource::new(queue.clone(), 48_000);
        assert!(source.frame(&metrics).1);
        notify(&metrics, event);
        assert_eq!(source.frame(&metrics), ([0.0; 2], false));
        assert!(
            queue.is_empty(),
            "unprocessed source retained stale PCM for event {event}"
        );
        assert_eq!(metrics.stream_resets.load(Ordering::Acquire), 1);
    }
}
