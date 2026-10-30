//! Exercise the native listener without CoreAudio calls or a physical device.
use super::*;
use std::sync::{atomic::AtomicBool, Barrier};

fn context() -> Capture {
    Capture {
        queue: Arc::new(ArrayQueue::new(1)),
        metrics: Arc::new(Metrics::default()),
        next_sample: AtomicU64::new(f64::NAN.to_bits()),
        gate: AtomicU64::new(0),
    }
}

fn notify(capture: &Capture, object: u32, addresses: &[ca::Address]) -> i32 {
    // SAFETY: the callback only borrows Capture, which outlives this invocation.
    // The slice supplies initialized addresses for the entire native callback.
    unsafe {
        configuration_event(
            object,
            addresses.len() as u32,
            addresses.as_ptr(),
            (capture as *const Capture).cast_mut().cast(),
        )
    }
}

#[test]
fn published_device_list_generation_always_includes_its_cause() {
    const EVENTS: u64 = 100_000;
    let capture = context();
    let start = Barrier::new(2);
    let done = AtomicBool::new(false);

    std::thread::scope(|threads| {
        let observer = threads.spawn(|| {
            start.wait();
            loop {
                let finished = done.load(Ordering::Acquire);
                let generation = capture.metrics.configuration_events.load(Ordering::Acquire);
                let causes = capture.metrics.device_list_events.load(Ordering::Acquire);
                // A subsequent callback may already have recorded its cause, so
                // equality would be too strict. A published generation must never
                // be ahead of the causes visible to an acquiring control thread.
                assert!(
                    causes >= generation,
                    "published generation {generation} exposed only {causes} topology causes"
                );
                if finished {
                    break;
                }
                std::hint::spin_loop();
            }
        });
        start.wait();
        let addresses = [ca::address(b"dev#")];
        let mut status = 0;
        for _ in 0..EVENTS {
            status |= notify(&capture, ca::SYSTEM_OBJECT, &addresses);
        }
        // Completion is independent of counter values, so a callback regression
        // cannot leave the observer waiting forever for a missing generation.
        done.store(true, Ordering::Release);
        observer.join().expect("configuration observer panicked");
        assert_eq!(status, 0);
    });

    assert_eq!(
        capture.metrics.configuration_events.load(Ordering::Acquire),
        EVENTS
    );
    assert_eq!(
        capture.metrics.device_list_events.load(Ordering::Acquire),
        EVENTS
    );
}

#[test]
fn only_system_device_list_notifications_publish_topology_causes() {
    let capture = context();
    assert_eq!(
        notify(&capture, ca::SYSTEM_OBJECT, &[ca::address(b"nsrt")]),
        0
    );
    assert_eq!(
        notify(&capture, ca::SYSTEM_OBJECT + 1, &[ca::address(b"dev#")]),
        0
    );
    assert_eq!(capture.metrics.device_list_events.load(Ordering::Acquire), 0);
    assert_eq!(capture.metrics.configuration_events.load(Ordering::Acquire), 2);

    // Several matching addresses still describe one listener notification.
    assert_eq!(
        notify(
            &capture,
            ca::SYSTEM_OBJECT,
            &[
                ca::address(b"nsrt"),
                ca::address(b"dev#"),
                ca::address(b"dev#"),
            ],
        ),
        0
    );
    assert_eq!(capture.metrics.device_list_events.load(Ordering::Acquire), 1);
    assert_eq!(capture.metrics.configuration_events.load(Ordering::Acquire), 3);
}

#[test]
fn empty_property_lists_do_not_invent_topology_changes() {
    let capture = context();
    assert_eq!(notify(&capture, ca::SYSTEM_OBJECT, &[]), 0);
    // SAFETY: a zero-length address list is represented by a null pointer; the
    // listener context remains valid and is only accessed through shared borrows.
    assert_eq!(
        unsafe {
            configuration_event(
                ca::SYSTEM_OBJECT,
                0,
                ptr::null(),
                (&capture as *const Capture).cast_mut().cast(),
            )
        },
        0
    );
    assert_eq!(capture.metrics.device_list_events.load(Ordering::Acquire), 0);
    assert_eq!(capture.metrics.configuration_events.load(Ordering::Acquire), 2);
}

#[test]
fn null_listener_context_is_a_noop() {
    let addresses = [ca::address(b"dev#")];
    // SAFETY: the address storage is valid and a null context must be ignored.
    assert_eq!(
        unsafe {
            configuration_event(
                ca::SYSTEM_OBJECT,
                addresses.len() as u32,
                addresses.as_ptr(),
                ptr::null_mut(),
            )
        },
        0
    );
}
