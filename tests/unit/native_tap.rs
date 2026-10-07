//! Native callback tests use owned PCM buffers, never system audio permission or physical devices.
use super::*;

fn context(capacity: usize) -> Capture {
    Capture {
        queue: Arc::new(ArrayQueue::new(capacity)),
        metrics: Arc::new(Metrics::default()),
        next_sample: AtomicU64::new(f64::NAN.to_bits()),
    }
}
fn interleaved(data: &mut [f32], channels: u32) -> ca::BufferList {
    ca::BufferList {
        count: 1,
        buffers: [ca::Buffer {
            channels,
            bytes: std::mem::size_of_val(data) as u32,
            data: data.as_mut_ptr().cast(),
        }],
    }
}
#[test]
fn clock_device_null_inputs_do_not_hide_the_tap_or_invent_microphone_audio() {
    #[repr(C)]
    struct ClockedList {
        count: u32,
        buffers: [ca::Buffer; 4],
    }
    let mut samples = [0.1_f32, -0.2, 0.3, -0.4];
    let mut list = ClockedList {
        count: 4,
        buffers: [
            ca::Buffer {
                channels: 8,
                bytes: 4096,
                data: ptr::null_mut(),
            },
            ca::Buffer {
                channels: 1,
                bytes: 4096,
                data: ptr::null_mut(),
            },
            ca::Buffer {
                channels: 2,
                bytes: 16,
                data: samples.as_mut_ptr().cast(),
            },
            ca::Buffer {
                channels: 2,
                bytes: 4096,
                data: ptr::null_mut(),
            },
        ],
    };
    let mut context = context(8);
    process((&list as *const ClockedList).cast(), &mut context);
    assert_eq!(context.queue.pop(), Some([0.1, -0.2]));
    assert_eq!(context.queue.pop(), Some([0.3, -0.4]));
    assert!(context.queue.is_empty());
    assert_eq!(context.metrics.errors.load(Ordering::Relaxed), 0);
    samples.fill(0.0);
    process((&list as *const ClockedList).cast(), &mut context);
    assert_eq!(context.queue.pop(), Some([0.0; 2]));
    assert_eq!(context.queue.pop(), Some([0.0; 2]));
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 4);
    list.buffers[2].data = ptr::null_mut();
    process((&list as *const ClockedList).cast(), &mut context);
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 4);
}

#[test]
fn unexpected_extra_active_inputs_fail_without_enqueuing_audio() {
    #[repr(C)]
    struct ExtraList {
        count: u32,
        buffers: [ca::Buffer; 3],
    }
    let mut samples = [0.25_f32; 2];
    let buffer = || ca::Buffer {
        channels: 1,
        bytes: 8,
        data: samples.as_ptr().cast_mut().cast(),
    };
    let list = ExtraList {
        count: 3,
        buffers: [buffer(), buffer(), buffer()],
    };
    let mut context = context(8);
    process((&list as *const ExtraList).cast(), &mut context);
    assert!(context.queue.is_empty());
    assert_eq!(context.metrics.errors.load(Ordering::Relaxed), 1);
    // Keep the owned allocation live through the callback.
    samples.fill(0.0);
}

#[test]
fn startup_requires_new_pcm_and_accepts_digital_silence() {
    let mut context = context(8);
    let mut samples = [0.0_f32; 4];
    let list = interleaved(&mut samples, 2);
    let zero = std::time::Duration::ZERO;
    assert!(wait_capture_ready(&context.metrics, 0, zero, || Ok(false)).is_err());
    process(&list, &mut context);
    wait_capture_ready(&context.metrics, 0, zero, || Ok(false)).unwrap();
    // Old PCM from an earlier stream does not acknowledge a fresh start.
    assert!(wait_capture_ready(&context.metrics, 2, zero, || Ok(false)).is_err());
    assert!(wait_capture_ready(&context.metrics, 0, zero, || Ok(true)).is_err());
    assert!(
        wait_capture_ready(&context.metrics, 0, zero, || anyhow::bail!("device gone")).is_err()
    );
    context.metrics.errors.store(1, Ordering::Release);
    assert!(wait_capture_ready(&context.metrics, 0, zero, || Ok(false)).is_err());
}

fn process(list: *const ca::BufferList, context: &mut Capture) {
    // SAFETY: each test passes buffers whose declared lengths and pointer lifetimes cover this call.
    let status = unsafe {
        capture(
            0,
            ptr::null(),
            list,
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            (context as *mut Capture).cast(),
        )
    };
    assert_eq!(status, 0);
}
fn process_at(list: *const ca::BufferList, context: &mut Capture, sample_time: f64, flags: u32) {
    let stamp = ca::TimeStamp {
        sample_time,
        flags,
        ..Default::default()
    };
    let status = unsafe {
        capture(
            0,
            ptr::null(),
            list,
            (&stamp as *const ca::TimeStamp).cast(),
            ptr::null_mut(),
            ptr::null(),
            (context as *mut Capture).cast(),
        )
    };
    assert_eq!(status, 0);
}

#[test]
fn native_timestamp_gaps_and_resets_are_counted_without_inventing_timestamps() {
    assert_eq!(std::mem::size_of::<ca::TimeStamp>(), 64);
    assert_eq!(std::mem::offset_of!(ca::TimeStamp, flags), 56);
    let mut context = context(32);
    let mut samples = [0.1_f32, -0.2, 0.3, -0.4];
    let list = interleaved(&mut samples, 2);
    process_at(&list, &mut context, 0.0, 1);
    process_at(&list, &mut context, 2.0, 1);
    assert_eq!(
        context
            .metrics
            .capture_discontinuities
            .load(Ordering::Relaxed),
        0
    );
    process_at(&list, &mut context, 100.0, 1);
    assert_eq!(
        context
            .metrics
            .capture_discontinuities
            .load(Ordering::Relaxed),
        1
    );
    process_at(&list, &mut context, 0.0, 1);
    assert_eq!(
        context
            .metrics
            .capture_discontinuities
            .load(Ordering::Relaxed),
        2
    );
    process_at(&list, &mut context, f64::NAN, 1);
    assert_eq!(
        context
            .metrics
            .capture_discontinuities
            .load(Ordering::Relaxed),
        3
    );
    process_at(&list, &mut context, 10000.0, 0);
    process_at(&list, &mut context, 0.0, 0);
    assert_eq!(
        context
            .metrics
            .capture_discontinuities
            .load(Ordering::Relaxed),
        3
    );
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 14);
}

#[test]
fn configuration_notification_blocks_capture_until_the_exact_generation_is_validated() {
    let mut context = context(8);
    let mut samples = [0.1_f32, -0.1];
    let list = interleaved(&mut samples, 2);
    let pointer = (&mut context as *mut Capture).cast();
    unsafe {
        configuration_event(0, 0, ptr::null(), pointer);
        configuration_event(0, 0, ptr::null(), pointer);
    }
    process(&list, &mut context);
    assert!(context.queue.is_empty());
    context
        .metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    process(&list, &mut context);
    assert!(context.queue.is_empty());
    context
        .metrics
        .checked_configuration_events
        .store(2, Ordering::Release);
    process(&list, &mut context);
    assert_eq!(context.queue.pop(), Some([0.1, -0.1]));
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 1);
}

#[test]
fn same_device_and_rate_do_not_hide_buffer_latency_or_liveness_changes() {
    let original = OutputConfiguration {
        rate: 48_000,
        buffer: Some(256),
        latency: Some(32),
        safety: Some(16),
        alive: Some(1),
    };
    for field in 0..5 {
        let mut changed = original.clone();
        match field {
            0 => changed.rate = 96_000,
            1 => changed.buffer = Some(512),
            2 => changed.latency = Some(64),
            3 => changed.safety = Some(32),
            _ => changed.alive = Some(0),
        }
        assert_ne!(original, changed);
    }
}

#[test]
fn follow_default_rebuilds_when_the_coreaudio_default_changes_or_temporarily_disappears() {
    assert!(!default_output_changed(None, Some(42)));
    assert!(!default_output_changed(Some(42), Some(42)));
    assert!(default_output_changed(Some(42), Some(77)));
    assert!(default_output_changed(Some(42), None));
}

#[test]
fn process_selection_rejects_empty_duplicate_self_and_stale_objects() {
    let available = [10_u32, 20, 30];
    assert!(validate_process_set(&[], &available, 99).is_err());
    assert!(validate_process_set(&[10, 10], &available, 99).is_err());
    assert!(validate_process_set(&[99], &available, 99).is_err());
    assert!(validate_process_set(&[40], &available, 99).is_err());
    validate_process_set(&[10, 30], &available, 99).unwrap();
}

#[test]
fn native_interleaved_channels_are_preserved() {
    let mut context = context(8);
    let mut samples = [0.1, -0.2, 0.3, -0.4];
    process(&interleaved(&mut samples, 2), &mut context);
    assert_eq!(context.queue.pop(), Some([0.1, -0.2]));
    assert_eq!(context.queue.pop(), Some([0.3, -0.4]));
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 2);
}
#[test]
fn native_planar_channels_are_preserved() {
    #[repr(C)]
    struct PlanarList {
        count: u32,
        buffers: [ca::Buffer; 2],
    }
    let mut left = [0.1_f32, 0.3];
    let mut right = [-0.2_f32, -0.4];
    let list = PlanarList {
        count: 2,
        buffers: [
            ca::Buffer {
                channels: 1,
                bytes: 8,
                data: left.as_mut_ptr().cast(),
            },
            ca::Buffer {
                channels: 1,
                bytes: 8,
                data: right.as_mut_ptr().cast(),
            },
        ],
    };
    let mut context = context(8);
    process((&list as *const PlanarList).cast(), &mut context);
    assert_eq!(context.queue.pop(), Some([0.1, -0.2]));
    assert_eq!(context.queue.pop(), Some([0.3, -0.4]));
}
#[test]
fn native_mono_and_nonfinite_samples_are_safe() {
    let mut context = context(8);
    let mut samples = [0.25_f32, f32::NAN, f32::INFINITY];
    process(&interleaved(&mut samples, 1), &mut context);
    assert_eq!(context.queue.pop(), Some([0.25; 2]));
    assert_eq!(context.queue.pop(), Some([0.0; 2]));
    assert_eq!(context.queue.pop(), Some([0.0; 2]));
}
#[test]
fn native_truncated_frames_fail_without_enqueueing() {
    let mut context = context(8);
    let mut samples = [0.1_f32, 0.2, 0.3];
    process(&interleaved(&mut samples, 2), &mut context);
    assert!(context.queue.is_empty());
    assert_eq!(context.metrics.errors.load(Ordering::Relaxed), 1);
}
#[test]
fn native_queue_pressure_is_counted_without_growing() {
    let mut context = context(1);
    let mut samples = [0.1_f32, 0.2, 0.3];
    process(&interleaved(&mut samples, 1), &mut context);
    assert_eq!(context.queue.len(), 1);
    assert_eq!(context.metrics.overruns.load(Ordering::Relaxed), 2);
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 3);
}
#[test]
fn native_empty_callback_does_not_invent_audio() {
    let mut context = context(8);
    process(ptr::null(), &mut context);
    let list = ca::BufferList {
        count: 0,
        buffers: [ca::Buffer {
            channels: 0,
            bytes: 0,
            data: ptr::null_mut(),
        }],
    };
    process(&list, &mut context);
    assert!(context.queue.is_empty());
    assert_eq!(context.metrics.captured_frames.load(Ordering::Relaxed), 0);
}
