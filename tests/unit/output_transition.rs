use super::*;
use crate::profile::Profile;

#[test]
fn renderer_keeps_settings_pending_until_the_dsp_crossfade_finishes() {
    let updates = Arc::new(ArrayQueue::new(8));
    let metrics = Arc::new(Metrics::default());
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let mut renderer = Renderer::new(base, 48_000, updates.clone(), metrics.clone());
    let target = Settings::compile(&Profile::preset("bass").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    assert!(
        updates
            .push(Update {
                settings: target,
                revision: 1,
                music_revision: 0,
            })
            .is_ok(),
        "fresh renderer update queue unexpectedly rejected its first settings update"
    );

    let mut data = vec![0.0_f32; 480 * 2];
    renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 1);
    assert!(
        metrics.settings_transitioning.load(Ordering::Relaxed),
        "runtime would report Applied while the DSP crossfade is still active"
    );

    for _ in 0..20 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(
        !metrics.settings_transitioning.load(Ordering::Relaxed),
        "completed DSP transition remained reported as pending"
    );
}

#[test]
fn renderer_keeps_reversed_crossfade_pending_until_return_finishes() {
    let updates = Arc::new(ArrayQueue::new(8));
    let metrics = Arc::new(Metrics::default());
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let target = Settings::compile(&Profile::preset("bass").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let mut renderer = Renderer::new(base, 48_000, updates.clone(), metrics.clone());
    let mut data = vec![0.0_f32; 480 * 2];

    assert!(updates
        .push(Update {
            settings: target,
            revision: 1,
            music_revision: 0,
        })
        .is_ok());
    for _ in 0..4 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(metrics.settings_transitioning.load(Ordering::Relaxed));

    assert!(updates
        .push(Update {
            settings: base,
            revision: 2,
            music_revision: 0,
        })
        .is_ok());
    renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 2);
    assert!(
        metrics.settings_transitioning.load(Ordering::Relaxed),
        "runtime reported Applied before the reversed DSP crossfade returned to baseline"
    );

    for _ in 0..3 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(
        !metrics.settings_transitioning.load(Ordering::Relaxed),
        "reversed DSP crossfade remained reported pending after returning to baseline"
    );
}

#[test]
fn renderer_keeps_third_target_pending_across_both_crossfades() {
    let updates = Arc::new(ArrayQueue::new(8));
    let metrics = Arc::new(Metrics::default());
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let middle = Settings::compile(&Profile::preset("bass").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let final_target = Settings::compile(&Profile::preset("clarity").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let mut renderer = Renderer::new(base, 48_000, updates.clone(), metrics.clone());
    let mut data = vec![0.0_f32; 480 * 2];

    assert!(updates
        .push(Update {
            settings: middle,
            revision: 1,
            music_revision: 0,
        })
        .is_ok());
    for _ in 0..4 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }

    assert!(updates
        .push(Update {
            settings: final_target,
            revision: 2,
            music_revision: 0,
        })
        .is_ok());
    renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 2);
    assert!(
        metrics.settings_transitioning.load(Ordering::Relaxed),
        "queued third target was reported Applied while the first crossfade was still active"
    );

    for _ in 0..7 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(
        metrics.settings_transitioning.load(Ordering::Relaxed),
        "runtime dropped pending state between the completed first crossfade and queued second crossfade"
    );

    for _ in 0..12 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(
        !metrics.settings_transitioning.load(Ordering::Relaxed),
        "queued third target remained pending after both DSP crossfades completed"
    );
}

#[test]
fn reselecting_inflight_target_cancels_older_third_target_without_restarting() {
    let updates = Arc::new(ArrayQueue::new(8));
    let metrics = Arc::new(Metrics::default());
    let base = Settings::compile(&Profile::default(), 48_000).unwrap();
    let target = Settings::compile(&Profile::preset("bass").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let obsolete = Settings::compile(&Profile::preset("clarity").unwrap(), 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let mut renderer = Renderer::new(base, 48_000, updates.clone(), metrics.clone());
    let mut data = vec![0.0_f32; 480 * 2];

    assert!(updates
        .push(Update {
            settings: target,
            revision: 1,
            music_revision: 0,
        })
        .is_ok());
    for _ in 0..4 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }

    assert!(updates
        .push(Update {
            settings: obsolete,
            revision: 2,
            music_revision: 0,
        })
        .is_ok());
    renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    assert!(metrics.settings_transitioning.load(Ordering::Relaxed));

    assert!(updates
        .push(Update {
            settings: target,
            revision: 3,
            music_revision: 0,
        })
        .is_ok());
    renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 3);
    assert!(metrics.settings_transitioning.load(Ordering::Relaxed));

    // Six blocks remained after re-selecting B. One extra block proves the
    // obsolete C target was cancelled rather than starting a second crossfade.
    for _ in 0..7 {
        renderer.render(&mut data, 2, || ([0.05, -0.04], true));
    }
    assert!(
        !metrics.settings_transitioning.load(Ordering::Relaxed),
        "re-selecting the in-flight target failed to cancel the older queued target"
    );
}
