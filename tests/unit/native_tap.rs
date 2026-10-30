//! Native callback tests use owned PCM buffers, never system audio permission or physical devices.
use super::*;

#[test]
fn internal_aggregate_churn_preserves_external_topology_but_hotplug_does_not() {
    let speaker = (7, Some("BuiltInSpeakerDevice".to_owned()));
    let microphone = (8, Some("BuiltInMicrophoneDevice".to_owned()));
    let original = device_topology([speaker.clone(), microphone.clone()]);
    let first = (90, Some("audio.maris.tap.first".to_owned()));
    let second = (91, Some("audio.maris.tap.second".to_owned()));
    for internal in [vec![], vec![first.clone()], vec![second.clone(), first]] {
        let mut devices = internal;
        devices.extend([microphone.clone(), speaker.clone()]);
        assert_eq!(device_topology(devices), original);
    }
    assert_ne!(device_topology([speaker.clone()]), original);
    assert_ne!(
        device_topology([
            speaker.clone(),
            microphone.clone(),
            (12, Some("USB".into()))
        ]),
        original
    );
    assert_ne!(
        device_topology([speaker.clone(), (8, Some("replacement".into()))]),
        original
    );
    // Unknown and non-Maris private devices cannot be silently ignored.
    assert_ne!(
        device_topology([speaker, microphone, (92, None), second]),
        original
    );
}

#[test]
fn output_only_clock_does_not_require_an_input_property() {
    let missing = ca::read_optional_input_streams(false, || {
        panic!("An absent input property must never be queried")
    })
    .unwrap();
    assert!(missing.is_empty());
    assert_eq!(
        ca::read_optional_input_streams(true, || Ok(vec![10, 11])).unwrap(),
        vec![10, 11]
    );
    // A driver read failure cannot hide physical microphone streams.
    let error =
        ca::read_optional_input_streams(true, || anyhow::bail!("driver failed")).unwrap_err();
    assert_eq!(error.to_string(), "driver failed");
}

#[test]
fn input_stream_scope_matches_the_coreaudio_abi() {
    let streams = ca::address_scope(b"stm#", ca::INPUT_SCOPE);
    let usage = ca::address_scope(b"suse", ca::INPUT_SCOPE);
    // SDK four-character code 'inpt', not 'inp '.
    assert_eq!(streams.scope, 0x696e7074);
    assert_eq!(usage.scope, 0x696e7074);
    assert_eq!(streams.selector, 0x73746d23);
    assert_eq!(usage.selector, 0x73757365);
    assert_eq!(ca::MAX_DRIFT_COMPENSATION_QUALITY, 0x7f);
    // kAudioAggregateDevicePropertyMainSubDevice / ActiveSubDeviceList.
    assert_eq!(u32::from_be_bytes(*b"amst"), 0x616d7374);
    assert_eq!(u32::from_be_bytes(*b"agrp"), 0x61677270);
    assert_eq!(u32::from_be_bytes(*b"tdsc"), 0x74647363);
}

#[test]
fn aggregate_clock_validation_fails_closed_on_wrong_or_inactive_physical_output() {
    ca::verify_aggregate_clock_state("speaker.uid", &[7, 9], "speaker.uid", 7).unwrap();

    let wrong_main =
        ca::verify_aggregate_clock_state("other.uid", &[7, 9], "speaker.uid", 7).unwrap_err();
    assert!(wrong_main
        .to_string()
        .contains("unexpected main clock device"));

    let inactive =
        ca::verify_aggregate_clock_state("speaker.uid", &[9, 11], "speaker.uid", 7).unwrap_err();
    assert!(inactive.to_string().contains("is not active"));
}

#[test]
fn original_playback_restore_quarantines_replacement_before_native_update() {
    let metrics = Metrics::default();
    let error = quarantine_then_restore(&metrics, || {
        assert!(metrics.output_quarantined.load(Ordering::Acquire));
        anyhow::bail!("injected native restore failure")
    })
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("injected native restore failure"));
    assert!(metrics.output_quarantined.load(Ordering::Acquire));
}

#[test]
fn capture_start_window_scales_with_the_aggregate_buffer_period() {
    assert_eq!(
        capture_progress_timeout(Some(512), 48_000),
        std::time::Duration::from_secs(2)
    );
    let large = capture_progress_timeout(Some(65_536), 44_100);
    assert!(large > std::time::Duration::from_secs(4));
    assert!(large <= std::time::Duration::from_secs(5));
    assert_eq!(
        capture_progress_timeout(None, 48_000),
        std::time::Duration::from_secs(2)
    );
}

#[test]
fn capture_gate_blocks_new_callbacks_and_waits_for_existing_readers() {
    let gate = Arc::new(AtomicU64::new(0));
    let first = enter_capture(&gate).expect("initial callback should enter");
    assert_eq!(gate.load(Ordering::Acquire), CAPTURE_GATE_READER);

    let worker_gate = gate.clone();
    let worker = std::thread::spawn(move || {
        block_capture(&worker_gate, std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(worker_gate.load(Ordering::Acquire), CAPTURE_GATE_BLOCKED);
        assert!(enter_capture(&worker_gate).is_none());
        unblock_capture(&worker_gate);
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while gate.load(Ordering::Acquire) & CAPTURE_GATE_BLOCKED == 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(enter_capture(&gate).is_none());
    drop(first);
    worker.join().unwrap();
    assert_eq!(gate.load(Ordering::Acquire), 0);
    assert!(enter_capture(&gate).is_some());
}

fn context(capacity: usize) -> Capture {
    context_with_queue(Arc::new(ArrayQueue::new(capacity)))
}
fn context_with_queue(queue: Arc<ArrayQueue<[f32; 2]>>) -> Capture {
    Capture {
        queue,
        metrics: Arc::new(Metrics::default()),
        next_sample: AtomicU64::new(f64::NAN.to_bits()),
        gate: AtomicU64::new(0),
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
fn capture_preflight_rejects_error_arriving_during_configuration_validation() {
    let metrics = Metrics::default();
    let result = wait_capture_ready(&metrics, 0, std::time::Duration::ZERO, || {
        metrics.captured_frames.store(2, Ordering::Release);
        metrics.errors.store(1, Ordering::Release);
        Ok(false)
    });
    assert!(
        result.is_err(),
        "new PCM hid a capture error during validation"
    );
}

#[test]
fn capture_preflight_requires_the_exact_validated_configuration_generation() {
    let metrics = Metrics::default();
    metrics.captured_frames.store(2, Ordering::Release);
    let zero = std::time::Duration::ZERO;
    assert!(
        wait_capture_ready(&metrics, 0, zero, || {
            metrics.configuration_events.store(1, Ordering::Release);
            Ok(false)
        })
        .is_err(),
        "unvalidated notification was accepted as capture readiness"
    );
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    wait_capture_ready(&metrics, 0, zero, || Ok(false)).unwrap();
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
fn maximum_native_blocks_start_live_source_and_preserve_pending_audio() {
    use crate::audio::bridge::LiveSource;

    const FRAMES: usize = 65_536;
    const PENDING_FRAMES: usize = 16_384;
    let mut samples = vec![0.25_f32; FRAMES * 2];
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let mut context = context_with_queue(super::capture_queue());
        let mut source = LiveSource::system(context.queue.clone(), rate);
        for block in 1..=2 {
            // A legal schedule delivers an entire native block before the output
            // callback runs. The second block must also fit beside pending PCM.
            process(&interleaved(&mut samples, 2), &mut context);
            assert_eq!(context.metrics.errors.load(Ordering::Acquire), 0);
            assert_eq!(
                context.metrics.overruns.load(Ordering::Acquire),
                0,
                "rate={rate}, block={block}: a legal native block lost PCM"
            );
            assert_eq!(
                context.metrics.captured_frames.load(Ordering::Acquire),
                (FRAMES * block) as u64
            );
            let mut nonzero_pcm = false;
            for _ in 0..256 {
                let (frame, ready) = source.frame(&context.metrics);
                assert!(
                    ready,
                    "rate={rate}, block={block}: source never became ready"
                );
                assert!(frame.iter().all(|sample| sample.is_finite()));
                nonzero_pcm |= frame.iter().any(|sample| sample.abs() > 0.001);
            }
            assert!(
                nonzero_pcm,
                "rate={rate}, block={block}: valid PCM was discarded"
            );
            assert_eq!(context.metrics.stream_resets.load(Ordering::Acquire), 0);

            // Simulate the rest of a consumer period without filtering 64k more
            // samples. Retain more than the largest 50 ms system-source reserve
            // at 192 kHz so the following capture is not tested on an empty queue.
            while context.queue.len() > PENDING_FRAMES {
                context.queue.pop().unwrap();
            }
            assert_eq!(context.queue.len(), PENDING_FRAMES);
        }
    }
}

#[test]
fn undersized_native_queue_repeatedly_loses_startup_pcm_after_overrun_reset() {
    use crate::audio::bridge::LiveSource;

    const FRAMES: usize = 65_536;
    const OLD_CAPACITY: usize = 48_000;
    let mut context = context(OLD_CAPACITY);
    let mut source = LiveSource::system(context.queue.clone(), 48_000);
    let mut samples = vec![0.25_f32; FRAMES * 2];
    for block in 1..=2 {
        // Ablation: use the former production capacity with the same whole-block
        // schedule. Capture advances, but every output attempt flushes its PCM.
        process(&interleaved(&mut samples, 2), &mut context);
        assert_eq!(context.metrics.errors.load(Ordering::Acquire), 0);
        assert_eq!(context.queue.len(), OLD_CAPACITY);
        assert_eq!(
            context.metrics.captured_frames.load(Ordering::Acquire),
            (FRAMES * block) as u64
        );
        assert_eq!(
            context.metrics.overruns.load(Ordering::Acquire),
            ((FRAMES - OLD_CAPACITY) * block) as u64
        );
        assert_eq!(source.frame(&context.metrics), ([0.0; 2], false));
        assert!(
            context.queue.is_empty(),
            "overrun recovery did not drain stale PCM"
        );
        assert_eq!(
            context.metrics.stream_resets.load(Ordering::Acquire),
            block as u64
        );
        assert_eq!(source.frame(&context.metrics), ([0.0; 2], false));
    }
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
    assert_eq!(
        context
            .metrics
            .capture_callback_calls
            .load(Ordering::Relaxed),
        1
    );
}
