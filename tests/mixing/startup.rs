//! Exercises the production control-thread sequence without native devices or audio capture.
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::Ordering,
};

type Events = Rc<RefCell<Vec<String>>>;
struct Probe {
    id: usize,
    events: Events,
    outputs: Vec<Arc<Metrics>>,
    fail: Option<&'static str>,
    output_error: Option<&'static str>,
}
impl Probe {
    fn step(&self, step: &str) -> Result<()> {
        self.events.borrow_mut().push(format!("{step}:{}", self.id));
        if self.output_error == Some(step) {
            self.outputs[1].errors.store(1, Ordering::Release);
        }
        anyhow::ensure!(self.fail != Some(step), "injected {step} failure");
        Ok(())
    }
}
impl CaptureLifecycle for Probe {
    fn start(&mut self) -> Result<()> {
        self.step("start")
    }
    fn check(&mut self) -> Result<()> {
        self.step("check")
    }
    fn arm(&mut self) -> Result<()> {
        self.step("arm")
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        let silent = self
            .outputs
            .iter()
            .all(|m| m.output_quarantined.load(Ordering::Acquire));
        self.events
            .borrow_mut()
            .push(format!("restore:{}:silent={silent}", self.id));
    }
}
struct OutputProbe(Events);
impl Drop for OutputProbe {
    fn drop(&mut self) {
        self.0.borrow_mut().push("drop-output".into());
    }
}
fn fixture(count: usize) -> (Vec<Probe>, Vec<Arc<Metrics>>, Events) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let outputs: Vec<_> = (0..2)
        .map(|_| {
            let metrics = Arc::new(Metrics::default());
            metrics.callback_ready.store(true, Ordering::Release);
            metrics.output_quarantined.store(true, Ordering::Release);
            metrics
        })
        .collect();
    let captures = (0..count)
        .map(|id| Probe {
            id,
            events: events.clone(),
            outputs: outputs.clone(),
            fail: None,
            output_error: None,
        })
        .collect();
    (captures, outputs, events)
}
fn refs(outputs: &[Arc<Metrics>]) -> Vec<&Metrics> {
    outputs.iter().map(AsRef::as_ref).collect()
}
fn quarantined(outputs: &[Arc<Metrics>]) -> bool {
    outputs
        .iter()
        .all(|m| m.output_quarantined.load(Ordering::Acquire))
}

#[test]
fn all_preflight_and_identity_checks_finish_before_any_original_path_is_muted() {
    let (mut captures, outputs, events) = fixture(3);
    let checks = Cell::new(0);
    start(&mut captures, &refs(&outputs), true, || {
        checks.set(checks.get() + 1);
        assert!(quarantined(&outputs));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        &*events.borrow(),
        &[
            "start:0", "start:1", "start:2", "check:0", "check:1", "check:2", "arm:0", "arm:1",
            "arm:2",
        ]
    );
    assert_eq!(
        checks.get(),
        3,
        "configuration must be revalidated after capture startup and after arming"
    );
    assert!(outputs
        .iter()
        .all(|m| !m.output_quarantined.load(Ordering::Acquire)));
}

#[test]
fn missing_failed_or_unquarantined_output_rejects_capture_before_native_start() {
    for fault in 0..3 {
        let (mut captures, outputs, events) = fixture(2);
        match fault {
            0 => outputs[1].callback_ready.store(false, Ordering::Release),
            1 => outputs[1].errors.store(1, Ordering::Release),
            _ => outputs[1]
                .output_quarantined
                .store(false, Ordering::Release),
        }
        assert!(
            start(&mut captures, &refs(&outputs), true, || Ok(())).is_err(),
            "fault {fault}"
        );
        assert!(
            events.borrow().is_empty(),
            "native capture started on an invalid output"
        );
    }
}

#[test]
fn empty_output_set_cannot_authorize_muting_any_application() {
    let (mut captures, _outputs, events) = fixture(1);
    assert!(start(&mut captures, &[], true, || Ok(())).is_err());
    assert!(events.borrow().is_empty());
}

#[test]
fn configuration_change_during_preflight_never_arms_a_capture() {
    let (mut captures, outputs, events) = fixture(2);
    let checks = Cell::new(0);
    assert!(start(&mut captures, &refs(&outputs), true, || {
        checks.set(checks.get() + 1);
        anyhow::ensure!(checks.get() == 1, "configuration revision changed");
        Ok(())
    })
    .is_err());
    assert!(quarantined(&outputs));
    assert!(!events
        .borrow()
        .iter()
        .any(|event| event.starts_with("arm:")));
}

#[test]
fn failure_in_any_capture_phase_keeps_all_outputs_quarantined_for_rollback() {
    for phase in ["start", "check", "arm"] {
        for fail_at in 0..3 {
            let (mut captures, outputs, events) = fixture(3);
            captures[fail_at].fail = Some(phase);
            assert!(
                start(&mut captures, &refs(&outputs), true, || Ok(())).is_err(),
                "{phase}:{fail_at}"
            );
            assert!(quarantined(&outputs));
            assert_eq!(
                events.borrow().last().unwrap(),
                &format!("{phase}:{fail_at}")
            );
            if phase != "arm" {
                assert!(!events
                    .borrow()
                    .iter()
                    .any(|event| event.starts_with("arm:")));
            }
        }
    }
}

#[test]
fn output_failure_during_start_or_arm_stops_before_the_next_capture() {
    for phase in ["start", "arm"] {
        for failed in 0..3 {
            let (mut captures, outputs, events) = fixture(3);
            captures[failed].output_error = Some(phase);
            assert!(
                start(&mut captures, &refs(&outputs), true, || Ok(())).is_err(),
                "{phase}:{failed}"
            );
            assert!(quarantined(&outputs));
            assert_eq!(
                events.borrow().last().unwrap(),
                &format!("{phase}:{failed}")
            );
        }
    }
}

#[test]
fn rollback_quarantines_every_bus_before_restoring_apps_and_releasing_outputs() {
    let (mut captures, outputs, events) = fixture(2);
    for m in &outputs {
        m.output_quarantined.store(false, Ordering::Release);
    }
    let mut streams = vec![OutputProbe(events.clone()), OutputProbe(events.clone())];
    release(&mut captures, &mut streams, &outputs);
    assert!(captures.is_empty() && streams.is_empty());
    assert_eq!(
        &*events.borrow(),
        &[
            "restore:0:silent=true",
            "restore:1:silent=true",
            "drop-output",
            "drop-output",
        ]
    );
}

#[test]
fn configuration_changed_while_arming_is_rejected_before_outputs_are_opened() {
    let (mut captures, outputs, events) = fixture(2);
    let result = start(&mut captures, &refs(&outputs), true, || {
        anyhow::ensure!(
            !events
                .borrow()
                .iter()
                .any(|event| event.starts_with("arm:")),
            "configuration changed during native mute update"
        );
        Ok(())
    });
    assert!(events.borrow().iter().any(|event| event == "arm:1"));
    assert!(
        result.is_err(),
        "startup accepted a revision changed during arming"
    );
    assert!(quarantined(&outputs));
}

#[test]
fn output_failure_during_final_revision_check_never_opens_any_bus() {
    let (mut captures, outputs, events) = fixture(2);
    assert!(start(&mut captures, &refs(&outputs), true, || {
        if events.borrow().iter().any(|event| event == "arm:1") {
            outputs[1].errors.store(1, Ordering::Release);
        }
        Ok(())
    })
    .is_err());
    assert!(quarantined(&outputs));
}

#[test]
fn one_phase_backends_preserve_their_existing_start_contract() {
    let (mut captures, outputs, events) = fixture(2);
    for m in &outputs {
        m.output_quarantined.store(false, Ordering::Release);
    }
    let checks = Cell::new(0);
    start(&mut captures, &refs(&outputs), false, || {
        checks.set(checks.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(&*events.borrow(), &["start:0", "start:1"]);
    assert_eq!(checks.get(), 1);
}
