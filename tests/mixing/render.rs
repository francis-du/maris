use super::*;
use crate::mixer::MixerStrip;

fn config() -> MixerConfig {
    let mut config = MixerConfig {
        strips: vec![
            MixerStrip {
                id: "a".into(),
                source_device: Some("input:0".into()),
                sends: [1.0, 0.0],
                ..MixerStrip::default()
            },
            MixerStrip {
                id: "b".into(),
                source_device: Some("input:1".into()),
                sends: [0.0, 1.0],
                ..MixerStrip::default()
            },
        ],
        ..MixerConfig::default()
    };
    config.buses[0].output_device = Some("output:0".into());
    config.buses[1].output_device = Some("output:1".into());
    config
}
struct Fixture {
    graph: render::Graph,
    sources: [Arc<ArrayQueue<[f32; 2]>>; 2],
    secondary: Arc<ArrayQueue<[f32; 2]>>,
    changes: Arc<ArrayQueue<render::Update>>,
    revision: Arc<AtomicU64>,
    meters: [Arc<Metrics>; 2],
}
impl Fixture {
    fn new(config: &MixerConfig) -> Self {
        let sources = std::array::from_fn(|_| Arc::new(ArrayQueue::new(16_384)));
        let inputs = sources
            .iter()
            .map(|queue| {
                Some(render::Input::new(
                    queue.clone(),
                    48000,
                    Arc::new(Metrics::default()),
                ))
            })
            .collect();
        let secondary = Arc::new(ArrayQueue::new(8192));
        let changes = Arc::new(ArrayQueue::new(2));
        let revision = Arc::new(AtomicU64::new(u64::MAX));
        let meters = std::array::from_fn(|_| Arc::new(Metrics::default()));
        let initial = render::Update::compile(config, 1, 48000).unwrap();
        changes.push(initial).ok().unwrap();
        let graph = render::Graph::new(render::GraphConfig {
            inputs,
            initial,
            delay_ms: [config.buses[0].delay_ms, config.buses[1].delay_ms],
            rate: 48_000,
            primary: 0,
            sends: [None, Some(secondary.clone())],
            send_metrics: meters.clone(),
            updates: changes.clone(),
            revision: revision.clone(),
        })
        .unwrap();
        Self {
            graph,
            sources,
            secondary,
            changes,
            revision,
            meters,
        }
    }
    fn fill(&self, frames: impl Iterator<Item = [[f32; 2]; 2]>) {
        for frames in frames {
            for (queue, frame) in self.sources.iter().zip(frames) {
                queue.push(frame).ok().unwrap();
            }
        }
    }
}

#[test]
fn one_starving_input_recovers_without_restarting_the_other_output_bus() {
    let mut f = Fixture::new(&config());
    for _ in 0..1400 {
        f.sources[0].push([0.1; 2]).unwrap();
    }
    for _ in 0..7000 {
        f.sources[1].push([0.2; 2]).unwrap();
    }
    let mut previous = 0.0_f32;
    for index in 0..3000 {
        let (a, ready) = f.graph.frame();
        let b = f.secondary.pop().unwrap();
        assert!(ready, "live second input was treated as missing");
        if index > 500 {
            assert!(
                (a[0] - previous).abs() < 0.001,
                "starving strip stepped its output"
            );
            assert!((b[0] - 0.2).abs() < 0.0001);
        }
        previous = a[0];
    }
    assert!(previous.abs() < 1e-5);
    for _ in 0..3000 {
        f.sources[0].push([-0.1; 2]).unwrap();
    }
    for _ in 0..2000 {
        let (a, _) = f.graph.frame();
        let b = f.secondary.pop().unwrap();
        assert!((a[0] - previous).abs() < 0.001);
        assert!((b[0] - 0.2).abs() < 0.0001);
        previous = a[0];
    }
    assert!((previous + 0.1).abs() < 0.0001);
}

#[test]
fn master_callback_interruption_flushes_each_old_input_backlog_once() {
    let mut f = Fixture::new(&config());
    f.fill((0..6000).map(|_| [[0.1; 2], [0.2; 2]]));
    for _ in 0..1000 {
        f.graph.frame();
        f.secondary.pop();
    }
    f.meters[0]
        .callback_discontinuities
        .store(1, Ordering::Release);
    assert!(!f.graph.frame().1);
    assert!(f.sources.iter().all(|queue| queue.is_empty()));
    f.fill((0..2000).map(|_| [[-0.1; 2], [-0.2; 2]]));
    assert!(
        f.graph.frame().1,
        "unchanged epoch repeatedly discarded fresh data"
    );
}

#[test]
fn each_source_is_consumed_once_and_output_buses_remain_independent() {
    let mut f = Fixture::new(&config());
    f.fill((0..6000).map(|_| [[0.1, -0.1], [0.2, -0.2]]));
    f.graph.begin_block();
    assert_eq!(f.revision.load(Ordering::Acquire), 1);
    for i in 0..3000 {
        let (a, ready) = f.graph.frame();
        let b = f.secondary.pop().unwrap();
        assert!(ready);
        if i > 500 {
            assert!((a[0] - 0.1).abs() < 0.0001 && (a[1] + 0.1).abs() < 0.0001);
            assert!((b[0] - 0.2).abs() < 0.0001 && (b[1] + 0.2).abs() < 0.0001);
        }
    }
    assert_eq!(f.sources[0].len(), f.sources[1].len());
    assert_eq!(f.meters[1].overruns.load(Ordering::Relaxed), 0);
}

#[test]
fn secondary_send_overruns_are_batched_per_block_without_losing_frames() {
    let mut f = Fixture::new(&config());
    // Fill the normal queue so every additional secondary frame is dropped.
    while f.secondary.push([0.0; 2]).is_ok() {}
    f.fill((0..8).map(|_| [[0.1; 2], [0.2; 2]]));
    f.graph.begin_block();
    for _ in 0..8 {
        let _ = f.graph.frame();
    }
    assert_eq!(
        f.meters[1].overruns.load(Ordering::Relaxed),
        0,
        "send-overrun telemetry was updated per frame instead of at the block boundary"
    );
    f.graph.end_block();
    assert_eq!(f.meters[1].overruns.load(Ordering::Relaxed), 8);
}

#[test]
fn configured_bus_delay_is_bounded_and_does_not_delay_the_other_bus() {
    let mut config = config();
    config.buses[1].delay_ms = 1.0;
    let mut f = Fixture::new(&config);
    f.fill((0..7000).map(|i| {
        let impulse = if i == 1500 { 0.5 } else { 0.0 };
        [[impulse, impulse], [impulse, impulse]]
    }));
    f.graph.begin_block();
    let mut primary_frames = Vec::new();
    let mut secondary_frames = Vec::new();
    for _ in 0..2200 {
        let (primary, ready) = f.graph.frame();
        if ready {
            primary_frames.push(primary[0]);
            secondary_frames.push(f.secondary.pop().unwrap()[0]);
        }
    }
    let primary_impulse = primary_frames
        .iter()
        .position(|sample| sample.abs() > 0.25)
        .expect("primary impulse was rendered");
    let secondary_impulse = secondary_frames
        .iter()
        .position(|sample| sample.abs() > 0.25)
        .expect("secondary impulse was rendered");
    assert_eq!(
        secondary_impulse - primary_impulse,
        48,
        "1 ms delay at 48 kHz must add exactly 48 mix-rate frames"
    );

    config.buses[1].delay_ms = 500.1;
    assert!(config.validate().is_err());
}

#[test]
fn unready_capture_does_not_invent_output_or_fill_other_bus() {
    let mut f = Fixture::new(&config());
    for _ in 0..1000 {
        assert_eq!(f.graph.frame(), ([0.0, 0.0], false));
    }
    assert!(f.secondary.is_empty());
    assert_eq!(f.revision.load(Ordering::Acquire), u64::MAX);
}

#[test]
fn live_gain_and_mute_changes_acknowledge_at_a_block_boundary_and_slew() {
    let mut config = config();
    let mut f = Fixture::new(&config);
    f.fill((0..7000).map(|_| [[0.1, 0.1], [0.2, 0.2]]));
    f.graph.begin_block();
    for _ in 0..1000 {
        f.graph.frame();
        f.secondary.pop();
    }
    config.strips[0].mute = true;
    f.changes
        .push(render::Update::compile(&config, 2, 48000).unwrap())
        .ok()
        .unwrap();
    assert_eq!(f.revision.load(Ordering::Acquire), 1);
    f.graph.begin_block();
    assert_eq!(
        f.revision.load(Ordering::Acquire),
        1,
        "mixer revision advanced before its gain ramp finished"
    );
    let mut previous = 0.101;
    for i in 0..1300 {
        let (a, _) = f.graph.frame();
        if i < 1199 {
            assert_eq!(
                f.revision.load(Ordering::Acquire),
                1,
                "mixer revision advanced during its gain ramp at frame {i}"
            );
        }
        let b = f.secondary.pop().unwrap();
        assert!(a[0] <= previous + 1e-6);
        assert!((previous - a[0]).abs() < 0.002);
        assert!((b[0] - 0.2).abs() < 0.0001);
        if i == 1299 {
            assert_eq!(a, [0.0; 2]);
        }
        previous = a[0];
    }
    assert_eq!(
        f.revision.load(Ordering::Acquire),
        2,
        "mixer revision did not advance after the audible ramp completed"
    );
}

#[test]
fn strip_eq_revision_waits_for_the_strip_dsp_crossfade() {
    let mut config = config();
    let mut f = Fixture::new(&config);
    f.fill((0..9000).map(|i| {
        let x = 0.15 * (i as f32 * std::f32::consts::TAU * 1000.0 / 48000.0).sin();
        [[x, x], [x, x]]
    }));
    f.graph.begin_block();
    for _ in 0..1500 {
        f.graph.frame();
        f.secondary.pop();
    }
    assert_eq!(f.revision.load(Ordering::Acquire), 1);

    let mut eq = crate::profile::Profile::default();
    eq.bands[5].gain_db = -6.0;
    config.strips[0].eq = Some(eq);
    f.changes
        .push(render::Update::compile(&config, 2, 48000).unwrap())
        .ok()
        .unwrap();
    f.graph.begin_block();
    assert_eq!(
        f.revision.load(Ordering::Acquire),
        1,
        "strip EQ revision advanced before the DSP crossfade started"
    );
    for i in 0..1199 {
        f.graph.frame();
        f.secondary.pop();
        assert_eq!(
            f.revision.load(Ordering::Acquire),
            1,
            "strip EQ revision advanced during DSP crossfade at frame {i}"
        );
    }
    f.graph.frame();
    f.secondary.pop();
    assert_eq!(
        f.revision.load(Ordering::Acquire),
        2,
        "strip EQ revision did not advance when the DSP crossfade completed"
    );
}

#[test]
fn strip_eq_changes_only_its_assigned_bus() {
    let mut config = config();
    let mut eq = crate::profile::Profile::default();
    eq.bands[5].gain_db = -6.0;
    config.strips[0].eq = Some(eq);
    let mut f = Fixture::new(&config);
    f.fill((0..7000).map(|i| {
        let x = 0.2 * (i as f32 * std::f32::consts::TAU * 1000.0 / 48000.0).sin();
        [[x, x], [x, x]]
    }));
    f.graph.begin_block();
    let mut energy = [0.0_f64; 2];
    for i in 0..4000 {
        let (a, _) = f.graph.frame();
        let b = f.secondary.pop().unwrap();
        if i > 2000 {
            energy[0] += f64::from(a[0]).powi(2);
            energy[1] += f64::from(b[0]).powi(2);
        }
    }
    let difference = 10.0 * (energy[0] / energy[1]).log10();
    assert!((difference + 6.0).abs() < 0.1, "{difference}");
}

#[test]
fn strip_compressor_does_not_compress_other_buses() {
    let mut config = config();
    config.strips[0].compressor.enabled = true;
    config.strips[0].compressor.threshold_db = -30.0;
    config.strips[0].compressor.ratio = 4.0;
    let mut f = Fixture::new(&config);
    f.fill((0..7000).map(|_| [[0.3, 0.3], [0.3, 0.3]]));
    for i in 0..5000 {
        let (a, _) = f.graph.frame();
        let b = f.secondary.pop().unwrap();
        if i > 4000 {
            assert!(a[0] < 0.15);
            assert!((b[0] - 0.3).abs() < 0.001);
        }
    }
}

#[test]
fn missing_devices_ambiguous_routes_and_oversized_config_fail_before_capture() {
    let mut c = config();
    assert!(validate_routes(&c).is_ok());
    c.strips[1].source_device = Some("input:0".into());
    assert!(validate_routes(&c).is_err());
    c = config();
    c.buses[1].output_device = c.buses[0].output_device.clone();
    assert!(validate_routes(&c).is_err());
    assert!(validate_routes(&MixerConfig::default()).is_err());
    c = config();
    c.strips = vec![MixerStrip::default(); 17];
    assert!(render::Update::compile(&c, 0, 48000).is_err());
}

#[test]
fn source_or_output_changes_require_explicit_restart_but_gain_edits_do_not() {
    let c = config();
    let mut next = c.clone();
    next.strips[0].gain_db = -3.0;
    assert!(same_routes(&c, &next));
    next.strips[0].source_device = Some("input:7".into());
    assert!(!same_routes(&c, &next));
    next = c.clone();
    next.buses[0].output_device = Some("output:3".into());
    assert!(!same_routes(&c, &next));
    next = c.clone();
    next.buses[1].delay_ms = 12.5;
    assert!(!same_routes(&c, &next));
    next = c.clone();
    next.strips[0].speech_denoise = true;
    assert!(!same_routes(&c, &next));
}

#[test]
fn denied_mixer_start_does_not_open_devices_or_create_state() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("absent"));
    assert!(Session::start(store.clone(), 48000, false).is_err());
    assert!(!store.directory.exists());
}
