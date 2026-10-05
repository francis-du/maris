use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACK_ALLOCATIONS.with(|tracking| {
            if tracking.get() {
                ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        TRACK_ALLOCATIONS.with(|tracking| {
            if tracking.get() {
                ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        TRACK_ALLOCATIONS.with(|tracking| {
            if tracking.get() {
                ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn allocations_during(run: impl FnOnce()) -> usize {
    ALLOCATION_COUNT.with(|count| count.set(0));
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(true));
    run();
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(false));
    ALLOCATION_COUNT.with(Cell::get)
}

#[test]
fn audio_processing_and_retune_crossfade_do_not_heap_allocate() {
    let profile = Profile::preset("bass").unwrap();
    let music = MusicProfile::preset("detail").unwrap();
    let settings = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap();
    let mut processor = Processor::new(settings);
    for i in 0..8_192 {
        let x = (0.1 * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin()) as f32;
        let _ = processor.process([x, -x * 0.3]);
    }

    let steady_allocations = allocations_during(|| {
        for i in 0..48_000 {
            let x = (0.1 * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin()) as f32;
            std::hint::black_box(processor.process([x, -x * 0.3]));
        }
    });
    assert_eq!(
        steady_allocations, 0,
        "steady audio processing allocated on the callback path"
    );

    let mut next_music = music.clone();
    next_music.bass_db = -3.0;
    next_music.air_db = 2.0;
    let next = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&next_music, 48_000)
        .unwrap();
    let transition_allocations = allocations_during(|| {
        processor.update(next);
        for i in 0..4_096 {
            let x = (0.08 * (std::f64::consts::TAU * 1_700.0 * i as f64 / 48_000.0).sin()) as f32;
            std::hint::black_box(processor.process([x, x * 0.2]));
        }
    });
    assert_eq!(
        transition_allocations, 0,
        "settings update/crossfade allocated on the callback path"
    );
}

#[test]
fn music_processor_steady_state_does_not_heap_allocate() {
    let mut profile = MusicProfile::preset("detail").unwrap();
    profile.bass_assist.enabled = true;
    profile.bass_assist.amount = 0.5;
    profile.virtual_surround = 0.5;
    profile.compressor.enabled = true;
    let mut processor = maris::music::Processor::new(profile.compile(48_000).unwrap());
    for i in 0..8_192 {
        let x = 0.12 * (std::f64::consts::TAU * 2_300.0 * i as f64 / 48_000.0).sin();
        let _ = processor.process([x, -x * 0.4]);
    }

    let allocations = allocations_during(|| {
        for i in 0..48_000 {
            let x = 0.12 * (std::f64::consts::TAU * 2_300.0 * i as f64 / 48_000.0).sin();
            std::hint::black_box(processor.process([x, -x * 0.4]));
        }
    });
    assert_eq!(
        allocations, 0,
        "music processing allocated on the callback path"
    );
}

#[test]
fn rapid_queued_retunes_do_not_heap_allocate() {
    let profile = Profile::default();
    let base_music = MusicProfile::preset("warm").unwrap();
    let mut second_music = MusicProfile::preset("detail").unwrap();
    second_music.virtual_surround = 0.6;
    let mut third_music = MusicProfile::preset("natural").unwrap();
    third_music.bass_assist.enabled = true;
    third_music.bass_assist.amount = 0.4;

    let base = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&base_music, 48_000)
        .unwrap();
    let second = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&second_music, 48_000)
        .unwrap();
    let third = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&third_music, 48_000)
        .unwrap();

    let mut processor = Processor::new(base);
    let allocations = allocations_during(|| {
        processor.update(second);
        for i in 0..320 {
            let x = (0.08 * (std::f64::consts::TAU * 1_100.0 * i as f64 / 48_000.0).sin()) as f32;
            std::hint::black_box(processor.process([x, -x * 0.25]));
        }

        processor.update(base);
        for i in 0..240 {
            let x = (0.08 * (std::f64::consts::TAU * 1_100.0 * i as f64 / 48_000.0).sin()) as f32;
            std::hint::black_box(processor.process([x, -x * 0.25]));
        }

        processor.update(second);
        processor.update(third);
        for i in 0..4_096 {
            let x = (0.08 * (std::f64::consts::TAU * 1_100.0 * i as f64 / 48_000.0).sin()) as f32;
            std::hint::black_box(processor.process([x, -x * 0.25]));
        }
    });

    assert_eq!(
        allocations, 0,
        "rapid/reversed/queued retunes allocated on the callback path"
    );
}
