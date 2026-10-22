//! Cross-stream configuration changes with the real graph/renderers and generated PCM.
use super::*;

fn renderer(fixture: &Fixture, bus: usize) -> bridge::Renderer {
    bridge::Renderer::new(
        Settings::compile(&crate::profile::Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        fixture.meters[bus].clone(),
    )
    .with_capture_configurations(&fixture.input_meters)
}

fn warmup(
    fixture: &mut Fixture,
    primary: &mut bridge::Renderer,
    secondary: &mut bridge::Renderer,
    source: &mut bridge::LiveSource,
) -> Instant {
    let now = Instant::now();
    fixture.fill((0..7000).map(|_| [[0.3; 2], [0.4; 2]]));
    let mut a = [0.0_f32; 4800];
    fixture.graph.begin_block();
    primary.render_at(&mut a, 2, || fixture.graph.frame(), now);
    fixture.graph.end_block();
    let mut b = [0.0_f32; 2400];
    secondary.render_at(&mut b, 2, || source.frame(&fixture.meters[1]), now);
    assert!(a.iter().any(|x| x.abs() > 0.01));
    assert!(b.iter().any(|x| x.abs() > 0.01));
    assert!(!fixture.secondary.is_empty());
    now
}

#[test]
fn stream_fault_spanning_a_graph_reset_does_not_publish_discarded_block_telemetry() {
    for input_fault in [false, true] {
        let mut f = Fixture::new(&config());
        let mut primary = renderer(&f, 0);
        let mut secondary = renderer(&f, 1);
        let mut source =
            bridge::LiveSource::with_rates(f.secondary.clone(), 48_000, 48_000).unwrap();
        let now = warmup(&mut f, &mut primary, &mut secondary, &mut source);
        let failed = if input_fault {
            f.input_meters[0].clone()
        } else {
            f.meters[0].clone()
        };
        let mut index = 0;
        let mut pcm = [1.0_f32; 256];
        f.graph.begin_block();
        primary.render_at(
            &mut pcm,
            2,
            || {
                if index == 64 {
                    bridge::output_stream_error(&failed);
                }
                index += 1;
                f.graph.frame()
            },
            now + Duration::from_millis(50),
        );
        f.graph.end_block();
        assert_eq!(
            index, 128,
            "the renderer must span the injected native error"
        );
        assert!(pcm.iter().all(|sample| *sample == 0.0));
        assert!(!f.meters[0].source_started.load(Ordering::Acquire));
        assert!(f
            .input_meters
            .iter()
            .all(|m| !m.source_started.load(Ordering::Acquire)));
        assert!(f
            .input_peaks
            .iter()
            .all(|peak| peak.load(Ordering::Acquire) == 0));
        assert_eq!(failed.errors.load(Ordering::Acquire), 1);
    }
}

#[test]
fn pending_input_configuration_silences_both_outputs_without_consuming_queues() {
    let mut f = Fixture::new(&config());
    let mut primary = renderer(&f, 0);
    let mut secondary = renderer(&f, 1);
    let mut source = bridge::LiveSource::with_rates(f.secondary.clone(), 48_000, 48_000).unwrap();
    let now = warmup(&mut f, &mut primary, &mut secondary, &mut source);
    let sizes = [f.sources[0].len(), f.sources[1].len(), f.secondary.len()];
    f.input_meters[0]
        .configuration_events
        .store(1, Ordering::Release);
    let mut a = [1.0_f32; 256];
    let mut b = [1.0_f32; 256];
    f.graph.begin_block();
    primary.render_at(
        &mut a,
        2,
        || f.graph.frame(),
        now + Duration::from_millis(50),
    );
    f.graph.end_block();
    secondary.render_at(
        &mut b,
        2,
        || source.frame(&f.meters[1]),
        now + Duration::from_millis(25),
    );
    assert!(
        a.iter().chain(&b).all(|sample| *sample == 0.0),
        "input configuration did not quarantine every physical output"
    );
    assert_eq!(
        [f.sources[0].len(), f.sources[1].len(), f.secondary.len()],
        sizes
    );
    assert!(f
        .meters
        .iter()
        .chain(&f.input_meters)
        .all(|m| !m.source_started.load(Ordering::Acquire)));
}

#[test]
fn mid_block_acknowledged_input_change_cannot_publish_discarded_pcm_as_ready() {
    let mut f = Fixture::new(&config());
    let mut primary = renderer(&f, 0);
    let mut secondary = renderer(&f, 1);
    let mut source = bridge::LiveSource::with_rates(f.secondary.clone(), 48_000, 48_000).unwrap();
    let now = warmup(&mut f, &mut primary, &mut secondary, &mut source);
    let input = f.input_meters[0].clone();
    let mut index = 0;
    let mut a = [0.0_f32; 256];
    f.graph.begin_block();
    primary.render_at(
        &mut a,
        2,
        || {
            if index == 64 {
                input.configuration_events.store(1, Ordering::Release);
                input
                    .checked_configuration_events
                    .store(1, Ordering::Release);
            }
            index += 1;
            f.graph.frame()
        },
        now + Duration::from_millis(50),
    );
    f.graph.end_block();
    assert_eq!(
        index, 128,
        "the actual callback must span the injected event"
    );
    assert!(a.iter().all(|sample| *sample == 0.0));
    assert!(
        f.input_meters
            .iter()
            .all(|m| !m.source_started.load(Ordering::Acquire)),
        "discarded graph block published input readiness"
    );
    assert!(
        f.input_peaks
            .iter()
            .all(|peak| peak.load(Ordering::Acquire) == 0),
        "discarded graph block retained audible peak telemetry"
    );
    let mut b = [0.0_f32; 256];
    secondary.render_at(
        &mut b,
        2,
        || source.frame(&f.meters[1]),
        now + Duration::from_millis(25),
    );
    assert!(
        b.iter().all(|sample| *sample == 0.0),
        "secondary replayed stale audio after the acknowledged event"
    );
}

#[test]
fn validated_input_change_discards_delayed_history_and_recovers_on_digital_silence() {
    let mut c = config();
    c.buses[0].delay_ms = 5.0;
    c.buses[1].delay_ms = 10.0;
    let mut f = Fixture::new(&c);
    let mut primary = renderer(&f, 0);
    let mut secondary = renderer(&f, 1);
    let mut source = bridge::LiveSource::with_rates(f.secondary.clone(), 48_000, 48_000).unwrap();
    let now = warmup(&mut f, &mut primary, &mut secondary, &mut source);
    f.input_meters[1]
        .configuration_events
        .store(1, Ordering::Release);
    let mut a = [0.0_f32; 256];
    let mut b = [0.0_f32; 256];
    f.graph.begin_block();
    primary.render_at(
        &mut a,
        2,
        || f.graph.frame(),
        now + Duration::from_millis(50),
    );
    f.graph.end_block();
    secondary.render_at(
        &mut b,
        2,
        || source.frame(&f.meters[1]),
        now + Duration::from_millis(25),
    );
    assert!(a.iter().chain(&b).all(|sample| *sample == 0.0));
    f.input_meters[1]
        .checked_configuration_events
        .store(1, Ordering::Release);
    f.graph.begin_block();
    primary.render_at(
        &mut a,
        2,
        || f.graph.frame(),
        now + Duration::from_millis(53),
    );
    f.graph.end_block();
    secondary.render_at(
        &mut b,
        2,
        || source.frame(&f.meters[1]),
        now + Duration::from_millis(28),
    );
    assert!(f.sources.iter().all(|q| q.is_empty()));
    assert!(f.secondary.is_empty());
    f.fill((0..4000).map(|_| [[0.0; 2], [0.0; 2]]));
    let mut a = [1.0_f32; 4800];
    let mut b = [1.0_f32; 2400];
    f.graph.begin_block();
    primary.render_at(
        &mut a,
        2,
        || f.graph.frame(),
        now + Duration::from_millis(56),
    );
    f.graph.end_block();
    secondary.render_at(
        &mut b,
        2,
        || source.frame(&f.meters[1]),
        now + Duration::from_millis(31),
    );
    assert!(
        a.iter().chain(&b).all(|sample| *sample == 0.0),
        "pre-change tail or bus delay survived recovery"
    );
    assert!(
        f.meters
            .iter()
            .chain(&f.input_meters)
            .all(|m| m.source_started.load(Ordering::Acquire)),
        "valid silent sources did not recover"
    );
}
