//! Finite generated sources through the production graph. No capture or physical devices.
use super::*;

const INPUT_FRAMES: usize = 1800;

struct Observation {
    primary: ([f32; 2], bool),
    secondary: Option<[f32; 2]>,
    input_ready: bool,
}

fn finite_source(delays: [usize; 2], silent: bool) -> Vec<Observation> {
    let mut c = config();
    for (bus, frames) in c.buses.iter_mut().zip(delays) {
        bus.delay_ms = frames as f64 / 48.0;
    }
    let mut f = Fixture::new(&c);
    f.fill((0..INPUT_FRAMES).map(|i| {
        let x = if silent {
            0.0
        } else {
            (std::f64::consts::TAU * 431.0 * i as f64 / 48_000.0).sin() as f32 * 0.2
        };
        [[x, -x], [x * 0.7, x * 0.3]]
    }));
    (0..INPUT_FRAMES + delays.into_iter().max().unwrap() + 1024)
        .map(|_| {
            f.graph.begin_block();
            let primary = f.graph.frame();
            f.graph.end_block();
            Observation {
                primary,
                secondary: f.secondary.pop(),
                input_ready: f
                    .input_meters
                    .iter()
                    .any(|m| m.source_started.load(Ordering::Acquire)),
            }
        })
        .collect()
}

#[test]
fn primary_delay_retains_ready_audio_after_all_sources_starve() {
    let baseline = finite_source([0, 0], false);
    let delayed = finite_source([2400, 0], false);
    let mut checked = 0;
    for (index, expected) in baseline
        .iter()
        .enumerate()
        .filter(|(_, frame)| frame.primary.1)
    {
        let actual = &delayed[index + 2400];
        assert!(
            actual.primary.1,
            "queued primary PCM lost readiness at mix frame {}",
            index + 2400
        );
        assert_eq!(
            actual.primary.0, expected.primary.0,
            "primary delay changed PCM"
        );
        checked += 1;
    }
    assert!(
        checked > 1000,
        "the reference must actually render the finite signal"
    );
}

#[test]
fn secondary_delay_is_enqueued_after_capture_stops_not_silently_discarded() {
    let baseline = finite_source([0, 0], false);
    let delayed = finite_source([0, 4800], false);
    let mut checked = 0;
    for (index, expected) in baseline.iter().enumerate() {
        if let Some(expected) = expected.secondary {
            assert_eq!(
                delayed[index + 4800].secondary,
                Some(expected),
                "secondary delayed PCM was lost at mix frame {}",
                index + 4800
            );
            checked += 1;
        }
    }
    assert!(checked > 1000);
}

#[test]
fn silent_pending_frames_drain_for_each_bus_delay_without_acknowledging_new_input() {
    let baseline = finite_source([0, 0], true);
    let delayed = finite_source([48, 24_000], true);
    let ready = baseline.iter().filter(|f| f.primary.1).count();
    assert!(ready > 1000);
    assert_eq!(delayed.iter().filter(|f| f.primary.1).count(), ready + 48);
    assert_eq!(
        delayed.iter().filter(|f| f.secondary.is_some()).count(),
        ready + 24_000
    );
    assert!(delayed.iter().any(|f| f.primary.1 && !f.input_ready));
    assert!(delayed
        .iter()
        .all(|f| f.primary.0 == [0.0; 2] && f.secondary.is_none_or(|pcm| pcm == [0.0; 2])));
    assert!(!delayed.last().unwrap().primary.1);
    assert!(delayed.last().unwrap().secondary.is_none());
}

#[test]
fn delay_without_any_ready_source_cannot_manufacture_audio_or_readiness() {
    let mut c = config();
    c.buses[0].delay_ms = 500.0;
    c.buses[1].delay_ms = 100.0;
    let mut f = Fixture::new(&c);
    for _ in 0..26_000 {
        assert_eq!(f.graph.frame(), ([0.0; 2], false));
        assert!(f.secondary.is_empty());
    }
}

#[test]
fn primary_discontinuity_discards_pending_delay_instead_of_draining_obsolete_audio() {
    let mut c = config();
    c.buses[0].delay_ms = 50.0;
    c.buses[1].delay_ms = 100.0;
    let mut f = Fixture::new(&c);
    f.fill((0..INPUT_FRAMES).map(|_| [[0.2; 2], [0.3; 2]]));
    for _ in 0..1000 {
        assert!(f.graph.frame().1);
        f.secondary.pop();
    }
    f.meters[0]
        .callback_discontinuities
        .fetch_add(1, Ordering::Release);
    for _ in 0..6000 {
        assert_eq!(f.graph.frame(), ([0.0; 2], false));
        assert!(
            f.secondary.is_empty(),
            "restart leaked pre-discontinuity delayed PCM"
        );
    }
}

#[test]
fn actual_output_renderer_delivers_a_signal_shorter_than_its_bus_delay() {
    let mut c = config();
    c.buses[0].delay_ms = 100.0;
    let mut f = Fixture::new(&c);
    f.fill((0..INPUT_FRAMES).map(|i| {
        let x = if (500..1000).contains(&i) { 0.2 } else { 0.0 };
        [[x; 2], [0.0; 2]]
    }));
    let mut output = bridge::Renderer::new(
        Settings::compile(&crate::profile::Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        f.meters[0].clone(),
    );
    let now = Instant::now();
    let mut pcm = [0.0_f32; 256];
    let mut audible_frames = 0;
    for block in 0..64 {
        f.graph.begin_block();
        output.render_at(
            &mut pcm,
            2,
            || f.graph.frame(),
            now + Duration::from_secs_f64(f64::from(block * 128) / 48_000.0),
        );
        f.graph.end_block();
        audible_frames += pcm
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|frame| frame[0].abs() > 0.05)
            .count();
        while f.secondary.pop().is_some() {}
    }
    assert!(
        audible_frames > 200,
        "the delayed finite signal never reached output PCM: {audible_frames}"
    );
    assert!(
        pcm.iter().all(|x| x.abs() < 1e-6),
        "the drained source never settled to silence"
    );
}
