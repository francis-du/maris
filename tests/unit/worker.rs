use super::*;
use std::sync::atomic::AtomicUsize;

struct Resource(Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn successful_worker_is_owned_and_drop_waits_for_resource_cleanup() {
    let released = Arc::new(AtomicUsize::new(0));
    let marker = released.clone();
    let worker = Worker::start(
        "owned-fixture",
        Duration::from_secs(2),
        move |_| Ok(Resource(marker)),
        |state, stop| {
            while !stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            drop(state);
        },
    )
    .unwrap();
    assert!(worker.healthy());
    assert_eq!(released.load(Ordering::SeqCst), 0);
    drop(worker);
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

#[test]
fn initialization_failure_never_enters_the_running_worker() {
    let released = Arc::new(AtomicUsize::new(0));
    let marker = released.clone();
    let worker = Worker::start::<Resource>(
        "failed-fixture",
        Duration::from_secs(2),
        move |_| {
            let _resource = Resource(marker);
            anyhow::bail!("fixture initialization failed")
        },
        |_, _| panic!("failed initialization entered run"),
    );
    assert!(worker.is_err());
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

#[test]
fn timed_out_startup_is_cancelled_and_cannot_run_later() {
    let released = Arc::new(AtomicUsize::new(0));
    let ran = Arc::new(AtomicBool::new(false));
    let marker = released.clone();
    let running = ran.clone();
    let worker = Worker::start(
        "late-fixture",
        Duration::from_millis(5),
        move |stop| {
            let state = Resource(marker);
            // Deterministically outlive the handshake, without relying on sleeps
            // or on the scheduler delivering initialization at an exact time.
            while !stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            Ok(state)
        },
        move |_, _| {
            running.store(true, Ordering::SeqCst);
        },
    );
    assert!(worker.is_err());
    assert_eq!(released.load(Ordering::SeqCst), 1);
    assert!(!ran.load(Ordering::SeqCst));
}

#[test]
fn panic_before_handshake_releases_state_and_reports_failure() {
    let released = Arc::new(AtomicUsize::new(0));
    let marker = released.clone();
    let worker = Worker::start::<Resource>(
        "panic-fixture",
        Duration::from_secs(2),
        move |_| {
            let _resource = Resource(marker);
            panic!("injected startup panic");
        },
        |_, _| panic!("panicked initialization entered run"),
    );
    assert!(worker.is_err());
    assert_eq!(released.load(Ordering::SeqCst), 1);
}
