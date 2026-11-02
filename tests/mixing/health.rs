//! Runtime failure containment with real renderer callbacks and generated PCM only.
use super::*;
use crate::{audio::bridge::Renderer, dsp::Settings, profile::Profile};
use crossbeam_queue::ArrayQueue;
use std::cell::Cell;

fn outputs() -> [Arc<Metrics>; 2] {
    std::array::from_fn(|_| Arc::new(Metrics::default()))
}
fn assert_quarantined(outputs: &[Arc<Metrics>]) {
    for metrics in outputs {
        assert!(metrics.output_quarantined.load(Ordering::Acquire));
        let settings = Settings::compile(&Profile::default(), 48_000).unwrap();
        let mut renderer = Renderer::new(
            settings,
            48_000,
            Arc::new(ArrayQueue::new(8)),
            metrics.clone(),
        );
        let mut pcm = [1.0_f32; 256];
        renderer.render(&mut pcm, 2, || {
            panic!("failed runtime still consumed capture")
        });
        assert!(pcm.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn fatal_saved_state_read_after_health_check_stops_and_quarantines() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::control::store::Store::at(directory.path().to_path_buf());
    std::fs::write(directory.path().join("mixer.json"), b"{invalid json").unwrap();
    let outputs = outputs();
    check(&[], &outputs, || true, || Ok(())).unwrap();
    let result = crate::mixer::load(&store).map(|_| true);
    let original = result.as_ref().unwrap_err().to_string();
    let mut stopped = false;
    let error = finish_tick(result, &mut stopped, &outputs).unwrap_err();
    assert_eq!(error.to_string(), original);
    assert!(stopped, "a fatal later state read left the session running");
    assert_quarantined(&outputs);
}

#[test]
fn fatal_runtime_publish_after_health_check_stops_and_quarantines() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("not-a-state-directory");
    std::fs::write(&blocked, b"owned test fixture").unwrap();
    let store = crate::control::store::Store::at(blocked);
    let outputs = outputs();
    check(&[], &outputs, || true, || Ok(())).unwrap();
    let result = store
        .write_json("runtime.json", &serde_json::json!({"active": true}))
        .map(|_| true);
    let original = result.as_ref().unwrap_err().to_string();
    let mut stopped = false;
    let error = finish_tick(result, &mut stopped, &outputs).unwrap_err();
    assert_eq!(error.to_string(), original);
    assert!(
        stopped,
        "failed status persistence left the session running"
    );
    assert_quarantined(&outputs);
}

#[test]
fn successful_tick_or_nonfatal_control_notice_keeps_healthy_audio_running() {
    let outputs = outputs();
    let mut stopped = false;
    assert!(finish_tick(Ok(true), &mut stopped, &outputs).unwrap());
    assert!(!stopped);
    assert!(outputs
        .iter()
        .all(|m| !m.output_quarantined.load(Ordering::Acquire)));
}

#[test]
fn graceful_stop_keeps_its_existing_fade_and_never_clears_stop_state() {
    let outputs = outputs();
    let mut stopped = true;
    assert!(!finish_tick(Ok(false), &mut stopped, &outputs).unwrap());
    assert!(stopped);
    assert!(outputs
        .iter()
        .all(|m| !m.output_quarantined.load(Ordering::Acquire)));
    outputs[0].output_quarantined.store(true, Ordering::Release);
    finish_tick(Ok(false), &mut stopped, &outputs).unwrap();
    assert!(outputs[0].output_quarantined.load(Ordering::Acquire));
}

#[test]
fn input_error_quarantines_all_outputs_before_return() {
    let inputs = [Arc::new(Metrics::default())];
    let outputs = outputs();
    inputs[0].errors.store(1, Ordering::Release);
    assert!(check(&inputs, &outputs, || true, || Ok(())).is_err());
    assert_quarantined(&outputs);
}

#[test]
fn output_error_quarantines_its_healthy_peer_before_return() {
    let outputs = outputs();
    outputs[1].errors.store(1, Ordering::Release);
    assert!(check(&[], &outputs, || true, || Ok(())).is_err());
    assert_quarantined(&outputs);
}

#[test]
fn configuration_failure_quarantines_without_losing_the_original_error() {
    let outputs = outputs();
    let error = check(
        &[],
        &outputs,
        || true,
        || anyhow::bail!("captured application identity changed"),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "captured application identity changed");
    assert_quarantined(&outputs);
}

#[test]
fn stream_failure_arriving_during_validation_is_still_contained() {
    let inputs = [Arc::new(Metrics::default())];
    let outputs = outputs();
    assert!(check(
        &inputs,
        &outputs,
        || true,
        || {
            inputs[0].errors.store(1, Ordering::Release);
            Ok(())
        }
    )
    .is_err());
    assert_quarantined(&outputs);
}

#[test]
fn empty_or_silent_source_is_not_a_worker_or_stream_failure() {
    let inputs = [Arc::new(Metrics::default())];
    let outputs = outputs();
    check(&inputs, &outputs, || true, || Ok(())).unwrap();
    assert!(outputs
        .iter()
        .all(|m| !m.output_quarantined.load(Ordering::Acquire)));
}

#[test]
fn successful_health_check_never_reopens_an_already_quarantined_output() {
    let outputs = outputs();
    outputs[0].output_quarantined.store(true, Ordering::Release);
    check(&[], &outputs, || true, || Ok(())).unwrap();
    assert!(outputs[0].output_quarantined.load(Ordering::Acquire));
}

#[test]
fn failed_speech_worker_is_fatal_even_when_output_callbacks_keep_advancing() {
    let outputs = outputs();
    for output in &outputs {
        output.callback_calls.store(100, Ordering::Release);
        output.frames.store(48_000, Ordering::Release);
    }
    let error = check(&[], &outputs, || false, || Ok(())).unwrap_err();
    assert!(error.to_string().contains("speech worker stopped"));
    assert_quarantined(&outputs);
}

#[test]
fn worker_ending_during_native_validation_is_checked_afterwards() {
    let outputs = outputs();
    let alive = Cell::new(true);
    assert!(check(
        &[],
        &outputs,
        || alive.get(),
        || {
            alive.set(false);
            Ok(())
        }
    )
    .is_err());
    assert_quarantined(&outputs);
}
