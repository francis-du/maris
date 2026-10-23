//! Configuration notifications injected into the actual renderer; no native audio.
use super::*;

#[path = "output_fault.rs"]
mod fault_tests;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct AllocationProbe;
impl AllocationProbe {
    fn count() {
        if TRACK.try_with(Cell::get).unwrap_or(false) {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    }
}
unsafe impl GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        Self::count();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;
fn allocations_during(run: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TRACK.with(|track| track.set(false));
        }
    }
    ALLOCATIONS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    let reset = Reset;
    run();
    drop(reset);
    ALLOCATIONS.with(Cell::get)
}

#[cfg(feature = "neural")]
#[test]
fn voice_recovery_handshake_never_allocates_on_the_rendering_thread() {
    let input = Arc::new(ArrayQueue::new(8192));
    for _ in 0..4 * 480 {
        input.push([0.0; 2]).unwrap();
    }
    let metrics = Arc::new(Metrics::default());
    let (worker, mut source) =
        crate::audio::voice::prepare(input, 48_000, 48_000, true, metrics.clone()).unwrap();
    let worker = worker.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while worker.output.len() != 4 * 480 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut renderer = renderer(metrics.clone());
    let start = Instant::now();
    let mut pcm = [0.0_f32; 256];
    let mut allocations = 0;
    for block in 0..80_u32 {
        if block == 4 {
            metrics
                .capture_discontinuities
                .fetch_add(1, Ordering::Release);
        }
        allocations += allocations_during(|| {
            renderer.render_at(
                &mut pcm,
                2,
                || source.frame(&metrics),
                start + Duration::from_secs_f64(f64::from(block * 128) / 48_000.0),
            );
        });
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        allocations, 0,
        "worker recovery allocated on the rendering thread"
    );
    let status = worker.status();
    assert!(status["recovery_generation"].as_u64().unwrap() >= 1);
    assert_eq!(status["recovery_pending"], false);
}

#[test]
fn guarded_rendering_is_sample_identical_to_unguarded_audio_without_notifications() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let captures: Vec<_> = (0..16).map(|_| Arc::new(Metrics::default())).collect();
        let settings = Settings::compile(&crate::profile::Profile::default(), rate).unwrap();
        let build = || {
            Renderer::new(
                settings,
                rate,
                Arc::new(ArrayQueue::new(8)),
                Arc::new(Metrics::default()),
            )
        };
        let mut guarded = build().with_capture_configurations(&captures);
        let mut baseline = build();
        let now = Instant::now();
        let mut a = [0.0_f32; 1024];
        let mut b = a;
        for block in 0..8 {
            let instant = now + Duration::from_secs_f64(f64::from(block * 512) / f64::from(rate));
            let mut i = block * 512;
            guarded.render_at(
                &mut a,
                2,
                || {
                    let x = (std::f64::consts::TAU * 997.0 * f64::from(i) / f64::from(rate)).sin()
                        as f32
                        * 0.3;
                    i += 1;
                    ([x, -x * 0.4], true)
                },
                instant,
            );
            let mut i = block * 512;
            baseline.render_at(
                &mut b,
                2,
                || {
                    let x = (std::f64::consts::TAU * 997.0 * f64::from(i) / f64::from(rate)).sin()
                        as f32
                        * 0.3;
                    i += 1;
                    ([x, -x * 0.4], true)
                },
                instant,
            );
            assert_eq!(
                a, b,
                "configuration guards changed unchanged PCM at {rate} Hz block {block}"
            );
        }
    }
}

#[test]
fn maximum_strip_guards_isolate_and_recover_without_callback_heap_allocations() {
    assert!(
        allocations_during(|| {
            std::hint::black_box(vec![0_u8; 31]);
        }) > 0,
        "the allocator probe must detect a known allocation"
    );
    let captures: Vec<_> = (0..16).map(|_| Arc::new(Metrics::default())).collect();
    let mut renderer =
        renderer(Arc::new(Metrics::default())).with_capture_configurations(&captures);
    let now = Instant::now();
    let mut data = [0.0_f32; 1024];
    let count = allocations_during(|| {
        for block in 0..20 {
            if block == 4 {
                captures[15]
                    .configuration_events
                    .store(1, Ordering::Release);
            }
            if block == 7 {
                captures[15]
                    .checked_configuration_events
                    .store(1, Ordering::Release);
            }
            renderer.render_at(
                &mut data,
                2,
                || ([0.3; 2], true),
                now + Duration::from_millis(block * 10),
            );
            std::hint::black_box(&data);
        }
    });
    assert_eq!(
        count, 0,
        "configuration isolation allocated on the audio callback"
    );
}

use crate::profile::Profile;

fn renderer(metrics: Arc<Metrics>) -> Renderer {
    Renderer::new(
        Settings::compile(&Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics,
    )
}

#[test]
fn pending_configuration_blocks_old_tail_and_does_not_consume_source() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = renderer(metrics.clone());
    let now = Instant::now();
    let mut warmup = [0.0_f32; 4800];
    renderer.render_at(&mut warmup, 2, || ([0.3; 2], true), now);
    assert!(warmup.iter().any(|sample| sample.abs() > 0.01));
    metrics.configuration_events.store(1, Ordering::Release);
    let mut consumed = 0;
    let mut data = [1.0_f32; 256];
    renderer.render_at(
        &mut data,
        2,
        || {
            consumed += 1;
            ([0.0; 2], false)
        },
        now + Duration::from_millis(50),
    );
    assert_eq!(
        consumed, 0,
        "an unchecked configuration consumed source frames"
    );
    assert!(
        data.iter().all(|sample| *sample == 0.0),
        "old held tail escaped configuration quarantine"
    );
    assert!(!metrics.source_started.load(Ordering::Acquire));
    assert_eq!(metrics.peak.load(Ordering::Acquire), 0.0_f32.to_bits());
}

fn inject_notification(acknowledge: bool) {
    for interrupt_at in 0..128 {
        let metrics = Arc::new(Metrics::default());
        let mut renderer = renderer(metrics.clone());
        let mut data = [0.0_f32; 256];
        let mut frame = 0;
        renderer.render(&mut data, 2, || {
            if frame == interrupt_at {
                metrics.configuration_events.store(1, Ordering::Release);
                if acknowledge {
                    // The control thread may validate the same configuration before
                    // the output callback returns. Its earlier PCM is still obsolete.
                    metrics
                        .checked_configuration_events
                        .store(1, Ordering::Release);
                }
            }
            frame += 1;
            ([0.5; 2], true)
        });
        assert!(frame > interrupt_at, "notification injection did not run");
        assert!(
            data.iter().all(|sample| *sample == 0.0),
            "notification at {interrupt_at}, acknowledged={acknowledge} leaked PCM"
        );
        assert!(!metrics.source_started.load(Ordering::Acquire));
    }
}

#[test]
fn mid_callback_configuration_notification_discards_inflight_pcm() {
    inject_notification(false);
}

#[test]
fn notification_acknowledged_inside_callback_still_discards_its_old_pcm() {
    inject_notification(true);
}

#[test]
fn acknowledged_notification_between_callbacks_cannot_reuse_the_previous_tail() {
    for previous in [0, u64::MAX] {
        let metrics = Arc::new(Metrics::default());
        metrics
            .configuration_events
            .store(previous, Ordering::Release);
        metrics
            .checked_configuration_events
            .store(previous, Ordering::Release);
        let mut renderer = renderer(metrics.clone());
        let now = Instant::now();
        let mut warmup = [0.0_f32; 4800];
        renderer.render_at(&mut warmup, 2, || ([0.3; 2], true), now);
        assert!(metrics.source_started.load(Ordering::Acquire));
        let next = previous.wrapping_add(1);
        metrics.configuration_events.store(next, Ordering::Release);
        metrics
            .checked_configuration_events
            .store(next, Ordering::Release);
        let mut data = [1.0_f32; 256];
        renderer.render_at(
            &mut data,
            2,
            || ([0.0; 2], false),
            now + Duration::from_millis(50),
        );
        assert!(
            data.iter().all(|sample| *sample == 0.0),
            "acknowledged generation change replayed a held tail"
        );
        assert!(!metrics.source_started.load(Ordering::Acquire));
        renderer.render_at(
            &mut data,
            2,
            || ([0.3; 2], true),
            now + Duration::from_millis(53),
        );
        let mut fresh = self::renderer(Arc::new(Metrics::default()));
        let mut expected = [0.0_f32; 256];
        fresh.render_at(
            &mut expected,
            2,
            || ([0.3; 2], true),
            now + Duration::from_millis(53),
        );
        assert_eq!(
            data, expected,
            "recovery must preserve the existing fresh-start envelope"
        );
    }
}

#[test]
fn configuration_recovery_flushes_old_live_pcm_before_accepting_fresh_silence() {
    let metrics = Arc::new(Metrics::default());
    let queue = Arc::new(ArrayQueue::new(8192));
    for _ in 0..6000 {
        queue.push([0.3; 2]).unwrap();
    }
    let mut source = LiveSource::with_rates(queue.clone(), 48_000, 48_000).unwrap();
    let mut renderer = renderer(metrics.clone());
    let now = Instant::now();
    let mut warmup = [0.0_f32; 4800];
    renderer.render_at(&mut warmup, 2, || source.frame(&metrics), now);
    assert!(warmup.iter().any(|sample| sample.abs() > 0.01));
    let retained = queue.len();
    metrics.configuration_events.store(1, Ordering::Release);
    let mut data = [1.0_f32; 256];
    renderer.render_at(
        &mut data,
        2,
        || source.frame(&metrics),
        now + Duration::from_millis(50),
    );
    assert!(data.iter().all(|sample| *sample == 0.0));
    assert_eq!(
        queue.len(),
        retained,
        "quarantined output consumed the live queue"
    );
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    renderer.render_at(
        &mut data,
        2,
        || source.frame(&metrics),
        now + Duration::from_millis(53),
    );
    assert!(
        queue.is_empty(),
        "the consumer did not flush pre-change PCM"
    );
    assert!(data.iter().all(|sample| *sample == 0.0));
    assert!(!metrics.source_started.load(Ordering::Acquire));
    for _ in 0..2000 {
        queue.push([0.0; 2]).unwrap();
    }
    renderer.render_at(
        &mut data,
        2,
        || source.frame(&metrics),
        now + Duration::from_millis(56),
    );
    assert!(data.iter().all(|sample| *sample == 0.0));
    assert!(
        metrics.source_started.load(Ordering::Acquire),
        "valid digital silence must prove readiness"
    );
}

#[test]
fn an_unchanged_acknowledged_configuration_does_not_block_audio_or_valid_silence() {
    let metrics = Arc::new(Metrics::default());
    metrics.configuration_events.store(7, Ordering::Release);
    metrics
        .checked_configuration_events
        .store(7, Ordering::Release);
    let mut renderer = renderer(metrics.clone());
    let now = Instant::now();
    let mut data = [0.0_f32; 256];
    renderer.render_at(&mut data, 2, || ([0.3; 2], true), now);
    assert!(data.iter().any(|sample| sample.abs() > 0.0));
    renderer.render_at(
        &mut data,
        2,
        || ([0.0; 2], true),
        now + Duration::from_millis(3),
    );
    assert!(metrics.source_started.load(Ordering::Acquire));
    assert_eq!(metrics.callback_discontinuities.load(Ordering::Acquire), 0);
}

#[test]
fn configuration_quarantine_uses_unsigned_format_silence_without_clearing_explicit_isolation() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = renderer(metrics.clone());
    metrics.configuration_events.store(1, Ordering::Release);
    let mut data = [0_u16; 128];
    renderer.render(&mut data, 1, || ([0.5; 2], true));
    assert!(data
        .iter()
        .all(|sample| *sample == u16::from_sample(0.0_f32)));
    metrics.output_quarantined.store(true, Ordering::Release);
    metrics
        .checked_configuration_events
        .store(1, Ordering::Release);
    renderer.render(&mut data, 1, || {
        panic!("configuration acknowledgement reopened an explicitly quarantined output")
    });
    assert!(metrics.output_quarantined.load(Ordering::Acquire));
}
