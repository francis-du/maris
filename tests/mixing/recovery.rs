//! Generated PCM only. Exercise the actual graph and secondary clock bridge, not hardware.
use super::*;

#[test]
fn primary_restart_invalidates_buffered_secondary_pcm_and_recovers_on_fresh_frames() {
    let mut f = Fixture::new(&config());
    f.fill((0..7000).map(|_| [[0.2; 2], [0.4; 2]]));
    for _ in 0..2400 {
        assert!(f.graph.frame().1);
    }
    let mut secondary =
        bridge::LiveSource::with_rates(f.secondary.clone(), 48_000, 48_000).unwrap();
    let mut previous = [0.0; 2];
    for _ in 0..512 {
        let (pcm, ready) = secondary.frame(&f.meters[1]);
        assert!(ready);
        previous = pcm;
    }
    assert!(previous[0] > 0.3, "the secondary must contain real old PCM");
    assert!(!f.secondary.is_empty());
    f.meters[0]
        .callback_discontinuities
        .fetch_add(1, Ordering::Release);
    f.graph.begin_block();
    assert!(!f.graph.frame().1);
    f.graph.end_block();
    let (pcm, ready) = secondary.frame(&f.meters[1]);
    assert!(
        !ready,
        "secondary replayed pre-restart PCM as current source"
    );
    assert_eq!(pcm, [0.0; 2]);
    assert!(
        f.secondary.is_empty(),
        "the consumer must discard its old backlog"
    );
    assert_eq!(f.meters[1].stream_resets.load(Ordering::Acquire), 1);

    f.fill((0..4000).map(|_| [[-0.2; 2], [-0.4; 2]]));
    for _ in 0..1800 {
        assert!(f.graph.frame().1);
    }
    // Observe beyond the unchanged 25 ms strip fade (1200 frames at 48 kHz)
    // and the secondary interpolator's filter history before asserting level.
    for _ in 0..1600 {
        let (pcm, ready) = secondary.frame(&f.meters[1]);
        assert!(ready, "unchanged epoch discarded fresh secondary PCM");
        previous = pcm;
    }
    assert!((previous[0] + 0.4).abs() < 0.001);
    assert_eq!(f.meters[1].stream_resets.load(Ordering::Acquire), 1);
}

#[test]
fn primary_restart_drops_each_strip_held_tail_before_new_digital_silence() {
    let mut c = config();
    c.buses[0].delay_ms = 5.0;
    c.buses[1].delay_ms = 10.0;
    let mut f = Fixture::new(&c);
    f.fill((0..7000).map(|_| [[0.3; 2], [0.4; 2]]));
    for _ in 0..1800 {
        f.graph.frame();
        f.secondary.pop();
    }
    f.meters[0]
        .callback_discontinuities
        .fetch_add(1, Ordering::Release);
    assert!(!f.graph.frame().1);
    f.fill((0..3000).map(|_| [[0.0; 2], [0.0; 2]]));
    for frame in 0..1600 {
        let (a, ready) = f.graph.frame();
        assert!(ready, "valid digital silence must remain ready");
        let b = f.secondary.pop().unwrap();
        assert_eq!(
            a, [0.0; 2],
            "old primary strip tail escaped at frame {frame}"
        );
        assert_eq!(
            b, [0.0; 2],
            "old secondary strip tail escaped at frame {frame}"
        );
    }
}

#[test]
fn secondary_only_discontinuity_does_not_reset_healthy_primary_or_inputs() {
    let mut f = Fixture::new(&config());
    f.fill((0..6000).map(|_| [[0.2; 2], [0.4; 2]]));
    for _ in 0..1000 {
        f.graph.frame();
        f.secondary.pop();
    }
    f.meters[1]
        .callback_discontinuities
        .fetch_add(1, Ordering::Release);
    let (pcm, ready) = f.graph.frame();
    assert!(ready);
    assert!((pcm[0] - 0.2).abs() < 0.001);
    assert!(f.sources.iter().all(|q| !q.is_empty()));
    assert!(f
        .input_meters
        .iter()
        .all(|m| m.callback_discontinuities.load(Ordering::Acquire) == 0));
    assert_eq!(
        f.meters[0].callback_discontinuities.load(Ordering::Acquire),
        0
    );
}

#[test]
fn primary_restart_notifies_secondary_once_without_reset_feedback() {
    let mut f = Fixture::new(&config());
    for epoch in 1..=3 {
        f.meters[0]
            .callback_discontinuities
            .store(epoch, Ordering::Release);
        f.graph.frame();
        assert_eq!(
            f.meters[1].callback_discontinuities.load(Ordering::Acquire),
            epoch
        );
        for _ in 0..100 {
            f.graph.frame();
        }
        assert_eq!(
            f.meters[0].callback_discontinuities.load(Ordering::Acquire),
            epoch
        );
        assert_eq!(
            f.meters[1].callback_discontinuities.load(Ordering::Acquire),
            epoch
        );
        assert!(f
            .input_meters
            .iter()
            .all(|m| m.callback_discontinuities.load(Ordering::Acquire) == epoch));
    }
}
