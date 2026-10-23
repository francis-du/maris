//! Faults injected into real renderers, without opening devices or invoking control ticks.
use super::*;

#[test]
fn previously_failed_output_does_not_consume_audio_or_report_ready() {
    let metrics = Arc::new(Metrics::default());
    let mut output = renderer(metrics.clone());
    let now = Instant::now();
    let mut warm = [0.0_f32; 2400];
    output.render_at(&mut warm, 2, || ([0.4; 2], true), now);
    assert!(warm.iter().any(|x| x.abs() > 0.01));
    metrics.errors.store(1, Ordering::Release);
    let mut consumed = 0;
    let mut pcm = [1.0_f32; 256];
    output.render_at(
        &mut pcm,
        2,
        || {
            consumed += 1;
            ([0.4; 2], true)
        },
        now + Duration::from_millis(25),
    );
    assert_eq!(
        consumed, 0,
        "failed stream consumed more source audio before the control tick"
    );
    assert!(pcm.iter().all(|x| *x == 0.0));
    assert!(!metrics.source_started.load(Ordering::Acquire));
    assert_eq!(metrics.peak.load(Ordering::Acquire), 0.0_f32.to_bits());
    assert_eq!(metrics.errors.load(Ordering::Acquire), 1);
}

#[test]
fn fault_arriving_inside_callback_discards_pcm_at_every_frame_boundary() {
    for capture_fault in [false, true] {
        for boundary in 0..128 {
            let metrics = Arc::new(Metrics::default());
            let capture = Arc::new(Metrics::default());
            let mut output = renderer(metrics.clone())
                .with_capture_configurations(std::slice::from_ref(&capture));
            let mut pcm = [0.0_f32; 256];
            let mut frames = 0;
            output.render(&mut pcm, 2, || {
                if frames == boundary {
                    let failed = if capture_fault { &capture } else { &metrics };
                    failed.errors.fetch_add(1, Ordering::Release);
                }
                frames += 1;
                ([0.5; 2], true)
            });
            assert!(frames > boundary, "injection must actually execute");
            assert!(
                pcm.iter().all(|x| *x == 0.0),
                "fault={capture_fault} boundary={boundary} escaped PCM"
            );
            assert!(!metrics.source_started.load(Ordering::Acquire));
        }
    }
}

#[test]
fn capture_error_silences_all_guarded_outputs_even_after_configuration_acknowledgement() {
    let capture = Arc::new(Metrics::default());
    capture.configuration_events.store(4, Ordering::Release);
    capture
        .checked_configuration_events
        .store(4, Ordering::Release);
    capture.errors.store(1, Ordering::Release);
    for _ in 0..2 {
        let metrics = Arc::new(Metrics::default());
        let mut output =
            renderer(metrics.clone()).with_capture_configurations(std::slice::from_ref(&capture));
        let mut consumed = 0;
        let mut pcm = [0_u16; 128];
        output.render(&mut pcm, 1, || {
            consumed += 1;
            ([0.5; 2], true)
        });
        assert_eq!(
            consumed, 0,
            "acknowledged configuration masked the capture error"
        );
        assert!(pcm.iter().all(|x| *x == u16::from_sample(0.0_f32)));
        assert!(!metrics.source_started.load(Ordering::Acquire));
        assert_eq!(
            metrics.errors.load(Ordering::Acquire),
            0,
            "do not copy/invent endpoint errors"
        );
    }
}

#[test]
fn fatal_guard_does_not_allocate_or_erase_explicit_quarantine() {
    let captures: Vec<_> = (0..16).map(|_| Arc::new(Metrics::default())).collect();
    let metrics = Arc::new(Metrics::default());
    let mut output = renderer(metrics.clone()).with_capture_configurations(&captures);
    captures[15].errors.store(1, Ordering::Release);
    let mut pcm = [1.0_f32; 256];
    let count =
        allocations_during(|| output.render(&mut pcm, 2, || panic!("faulted source was consumed")));
    assert_eq!(count, 0);
    assert!(pcm.iter().all(|x| *x == 0.0));
    metrics.output_quarantined.store(true, Ordering::Release);
    output.render(&mut pcm, 2, || panic!("explicit isolation was cleared"));
    assert!(metrics.output_quarantined.load(Ordering::Acquire));
}

#[test]
fn healthy_silence_and_recoverable_loss_counters_are_not_fatal_errors() {
    let metrics = Arc::new(Metrics::default());
    let capture = Arc::new(Metrics::default());
    capture.overruns.store(5, Ordering::Release);
    capture.underruns.store(7, Ordering::Release);
    let mut output = renderer(metrics.clone()).with_capture_configurations(&[capture]);
    let mut pcm = [1.0_f32; 256];
    output.render(&mut pcm, 2, || ([0.0; 2], true));
    assert!(pcm.iter().all(|x| *x == 0.0));
    assert!(metrics.source_started.load(Ordering::Acquire));
    assert_eq!(metrics.errors.load(Ordering::Acquire), 0);
}
