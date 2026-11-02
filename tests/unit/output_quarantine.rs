//! Deterministic mid-callback control events; no hardware, permissions or recorded audio.
use super::*;
use crate::profile::Profile;

fn renderer() -> (Renderer, Arc<Metrics>) {
    let metrics = Arc::new(Metrics::default());
    let settings = Settings::compile(&Profile::default(), 48_000).unwrap();
    let renderer = Renderer::new(
        settings,
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );
    (renderer, metrics)
}

#[test]
fn quarantine_during_render_discards_the_entire_inflight_callback() {
    for interrupt_at in 0..256 {
        let (mut renderer, metrics) = renderer();
        let mut data = [0.0_f32; 512];
        let mut frame = 0;
        renderer.render(&mut data, 2, || {
            if frame == interrupt_at {
                metrics.output_quarantined.store(true, Ordering::Release);
            }
            frame += 1;
            ([0.5, -0.5], true)
        });
        assert!(
            frame > interrupt_at,
            "the injected callback must actually execute"
        );
        assert!(
            data.iter().all(|sample| *sample == 0.0),
            "PCM escaped quarantine at frame {interrupt_at}"
        );
        assert!(!metrics.source_started.load(Ordering::Acquire));
        assert_eq!(f32::from_bits(metrics.peak.load(Ordering::Relaxed)), 0.0);
        assert_eq!(renderer.render.gain, 0.0);
        data.fill(1.0);
        renderer.render(&mut data, 2, || {
            panic!("quarantined output consumed the live source")
        });
        assert!(data.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn resumed_quarantine_does_not_replay_held_samples_or_skip_the_startup_fade() {
    let (mut renderer, metrics) = renderer();
    let start = Instant::now();
    let mut warmup = [0.0_f32; 4800];
    renderer.render_at(&mut warmup, 2, || ([0.3; 2], true), start);
    assert_eq!(renderer.render.gain, 1.0);
    assert!(warmup.iter().any(|sample| sample.abs() > 0.01));
    let mut data = [0.0_f32; 256];
    renderer.render_at(
        &mut data,
        2,
        || {
            metrics.output_quarantined.store(true, Ordering::Release);
            ([0.3; 2], true)
        },
        start + Duration::from_millis(50),
    );
    assert!(data.iter().all(|sample| *sample == 0.0));
    metrics.output_quarantined.store(false, Ordering::Release);
    renderer.render_at(
        &mut data,
        2,
        || ([0.0; 2], false),
        start + Duration::from_millis(53),
    );
    assert!(
        data.iter().all(|sample| *sample == 0.0),
        "quarantine left a held sample behind"
    );
    assert!(!metrics.source_started.load(Ordering::Acquire));
    renderer.render_at(
        &mut data,
        2,
        || ([0.3; 2], true),
        start + Duration::from_millis(56),
    );
    assert!(data[0].abs() < 0.001);
    // Compare with a fresh renderer, not an arbitrary threshold that changes the
    // established 25 ms startup envelope (128 frames advance it by about 0.107).
    let (mut fresh, _) = self::renderer();
    let mut expected = [0.0_f32; 256];
    fresh.render_at(
        &mut expected,
        2,
        || ([0.3; 2], true),
        start + Duration::from_millis(56),
    );
    assert_eq!(data, expected);
    assert_eq!(renderer.render.gain, fresh.render.gain);
}

#[test]
fn quarantine_before_a_callback_clears_old_readiness_and_peak_telemetry() {
    let (mut renderer, metrics) = renderer();
    let mut data = [0.0_f32; 512];
    renderer.render(&mut data, 2, || ([0.5; 2], true));
    assert!(metrics.source_started.load(Ordering::Acquire));
    metrics.output_quarantined.store(true, Ordering::Release);
    renderer.render(&mut data, 2, || {
        panic!("quarantined callback read the source")
    });
    assert!(!metrics.source_started.load(Ordering::Acquire));
    assert_eq!(metrics.peak.load(Ordering::Acquire), 0.0_f32.to_bits());
    assert_eq!(renderer.render.gain, 0.0);
}

fn integer_silence<T: SizedSample + FromSample<f32> + PartialEq + std::fmt::Debug>() {
    for channels in [1, 2] {
        let (mut renderer, metrics) = renderer();
        let mut data = vec![T::from_sample(0.5); 256];
        renderer.render(&mut data, channels, || {
            metrics.output_quarantined.store(true, Ordering::Release);
            ([0.5; 2], true)
        });
        assert!(data.iter().all(|sample| *sample == T::from_sample(0.0)));
    }
}

#[test]
fn quarantine_uses_the_correct_silence_for_all_supported_integer_formats() {
    integer_silence::<i16>();
    integer_silence::<u16>();
    integer_silence::<i32>();
}

#[test]
fn handoff_checks_configuration_errors_and_exact_notification_generations() {
    let metrics = Metrics::default();
    metrics.source_started.store(true, Ordering::Release);
    assert!(wait_source_started(&metrics, Duration::ZERO, || Ok(true)).is_err());
    assert!(
        wait_source_started(&metrics, Duration::ZERO, || anyhow::bail!(
            "device disappeared"
        ))
        .is_err()
    );
    metrics.configuration_events.store(1, Ordering::Release);
    assert!(wait_source_started(&metrics, Duration::ZERO, || Ok(false)).is_err());
    let mut checks = 0;
    wait_source_started(&metrics, Duration::from_secs(1), || {
        checks += 1;
        metrics
            .checked_configuration_events
            .store(checks, Ordering::Release);
        // Simulate a second notification arriving during the first validation.
        metrics.configuration_events.store(2, Ordering::Release);
        Ok(false)
    })
    .unwrap();
    assert_eq!(checks, 2);
}

#[test]
fn output_error_arriving_during_configuration_validation_still_rejects_handoff() {
    let metrics = Metrics::default();
    metrics.source_started.store(true, Ordering::Release);
    assert!(wait_source_started(&metrics, Duration::ZERO, || {
        metrics.errors.store(1, Ordering::Release);
        Ok(false)
    })
    .is_err());
}

#[test]
fn a_quarantined_renderer_cannot_acknowledge_handoff_from_stale_progress() {
    let metrics = Metrics::default();
    metrics.source_started.store(true, Ordering::Release);
    metrics.output_quarantined.store(true, Ordering::Release);
    assert!(wait_source_started(&metrics, Duration::ZERO, || Ok(false)).is_err());
}
