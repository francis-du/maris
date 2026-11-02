//! Production handoff verification driven by isolated counters; no native audio or permissions.
use super::*;
use std::{cell::Cell, time::Duration};

fn fixture() -> (Vec<Metrics>, Vec<Metrics>) {
    let inputs: Vec<_> = (0..2).map(|_| Metrics::default()).collect();
    let outputs: Vec<_> = (0..2).map(|_| Metrics::default()).collect();
    for input in &inputs {
        input.captured_frames.store(100, Ordering::Release);
        input.source_started.store(true, Ordering::Release);
    }
    for output in &outputs {
        output.callback_ready.store(true, Ordering::Release);
        output.source_started.store(true, Ordering::Release);
    }
    (inputs, outputs)
}
fn refs(metrics: &[Metrics]) -> Vec<&Metrics> {
    metrics.iter().collect()
}
fn advance(inputs: &[Metrics]) {
    for input in inputs {
        input.captured_frames.fetch_add(256, Ordering::Release);
    }
}
fn quarantined(outputs: &[Metrics]) -> bool {
    outputs
        .iter()
        .all(|m| m.output_quarantined.load(Ordering::Acquire))
}

#[test]
fn preflight_capture_and_old_renderer_flags_do_not_prove_post_takeover_audio() {
    let (inputs, outputs) = fixture();
    assert!(wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || Ok(())).is_err());
    assert!(quarantined(&outputs));
}

#[test]
fn every_input_needs_new_capture_and_its_own_graph_acknowledgement() {
    for missing in 0..2 {
        for capture_only in [false, true] {
            let (inputs, outputs) = fixture();
            let result = wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
                advance(&inputs);
                if capture_only {
                    inputs[missing]
                        .source_started
                        .store(false, Ordering::Release);
                } else {
                    inputs[missing]
                        .captured_frames
                        .store(100, Ordering::Release);
                }
                Ok(())
            });
            assert!(
                result.is_err(),
                "input={missing}, capture_only={capture_only}"
            );
            assert!(quarantined(&outputs));
        }
    }
}

#[test]
fn every_active_output_must_render_a_source_not_just_run_a_silent_callback() {
    for missing in 0..2 {
        let (inputs, outputs) = fixture();
        outputs[missing]
            .source_started
            .store(false, Ordering::Release);
        assert!(
            wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
                advance(&inputs);
                Ok(())
            })
            .is_err()
        );
        assert!(quarantined(&outputs));
    }
}

#[test]
fn valid_digital_silence_needs_no_nonzero_peak_to_confirm_takeover() {
    let (inputs, outputs) = fixture();
    wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
        advance(&inputs);
        Ok(())
    })
    .unwrap();
    assert!(outputs.iter().all(|m| m.peak.load(Ordering::Acquire) == 0));
    assert!(!quarantined(&outputs));
}

#[test]
fn configuration_identity_or_worker_failure_quarantines_before_returning() {
    let (inputs, outputs) = fixture();
    let error = wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
        advance(&inputs);
        anyhow::bail!("captured process identity changed")
    })
    .unwrap_err();
    assert!(error.to_string().contains("identity changed"));
    assert!(quarantined(&outputs));
}

#[test]
fn late_input_output_stop_and_quarantine_faults_cannot_be_hidden_by_ready_flags() {
    for fault in 0..4 {
        let (inputs, outputs) = fixture();
        let result = wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
            advance(&inputs);
            match fault {
                0 => inputs[1].errors.store(1, Ordering::Release),
                1 => outputs[1].errors.store(1, Ordering::Release),
                2 => inputs[1].stopping.store(true, Ordering::Release),
                _ => outputs[1].output_quarantined.store(true, Ordering::Release),
            }
            Ok(())
        });
        assert!(result.is_err(), "fault={fault}");
        assert!(quarantined(&outputs));
    }
}

#[test]
fn pending_generations_wait_for_validation_instead_of_accepting_old_flags() {
    for output_event in [false, true] {
        let (inputs, outputs) = fixture();
        let pending = if output_event {
            &outputs[1]
        } else {
            &inputs[1]
        };
        pending.configuration_events.store(1, Ordering::Release);
        let checks = Cell::new(0);
        wait_ready(
            &refs(&inputs),
            &refs(&outputs),
            Duration::from_secs(1),
            || {
                checks.set(checks.get() + 1);
                advance(&inputs);
                if checks.get() == 2 {
                    pending
                        .checked_configuration_events
                        .store(1, Ordering::Release);
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(checks.get(), 2);
        assert!(!quarantined(&outputs));
    }
}

#[test]
fn later_post_takeover_capture_is_checked_until_all_sources_are_ready() {
    let (inputs, outputs) = fixture();
    inputs[1].source_started.store(false, Ordering::Release);
    let checks = Cell::new(0);
    wait_ready(
        &refs(&inputs),
        &refs(&outputs),
        Duration::from_secs(1),
        || {
            checks.set(checks.get() + 1);
            inputs[0].captured_frames.fetch_add(256, Ordering::Release);
            if checks.get() == 3 {
                inputs[1].captured_frames.fetch_add(256, Ordering::Release);
                inputs[1].source_started.store(true, Ordering::Release);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(checks.get(), 3);
}

#[test]
fn empty_endpoint_sets_fail_and_counter_wrap_is_still_progress() {
    let (inputs, outputs) = fixture();
    assert!(wait_ready(&[], &refs(&outputs), Duration::ZERO, || Ok(())).is_err());
    assert!(quarantined(&outputs));
    assert!(wait_ready(&refs(&inputs), &[], Duration::ZERO, || Ok(())).is_err());
    let (inputs, outputs) = fixture();
    for input in &inputs {
        input.captured_frames.store(u64::MAX, Ordering::Release);
    }
    wait_ready(&refs(&inputs), &refs(&outputs), Duration::ZERO, || {
        advance(&inputs);
        Ok(())
    })
    .unwrap();
}
