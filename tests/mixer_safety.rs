use maris::{
    mixer::{self, MixerConfig, MixerStrip, Processor},
    store::Store,
};

#[test]
fn unchanged_mixer_edits_preserve_revision_and_undo() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let initial = mixer::edit(&store, Some(0), |config| {
        config.strips.push(MixerStrip::default());
        Ok(())
    })
    .unwrap();
    let changed = mixer::edit(&store, Some(initial.revision), |config| {
        config.strips[0].gain_db = -6.0;
        Ok(())
    })
    .unwrap();
    let bytes = std::fs::read(store.directory.join("mixer.json")).unwrap();
    let same = mixer::edit(&store, Some(changed.revision), |_| Ok(())).unwrap();
    assert_eq!(same, changed);
    assert_eq!(
        std::fs::read(store.directory.join("mixer.json")).unwrap(),
        bytes
    );
    let undo = mixer::undo(&store, Some(same.revision)).unwrap();
    assert_eq!(undo.config.strips[0].gain_db, 0.0);
}

#[test]
fn nonfinite_input_cannot_poison_either_mixer_bus() {
    let config = MixerConfig {
        strips: vec![MixerStrip {
            sends: [1.0, 1.0],
            ..MixerStrip::default()
        }],
        ..MixerConfig::default()
    };
    let mut engine = Processor::new(config, 48000).unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        let out = engine.process(&[[value, 0.1]]);
        assert!(out
            .iter()
            .flatten()
            .all(|x| x.is_finite() && x.abs() <= 0.892));
    }
    let out = engine.process(&[[0.1, 0.1]]);
    assert!((out[0][0] - 0.1).abs() < 0.0001);
}

#[test]
fn concurrent_scene_saves_do_not_drop_other_scenes() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|index| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                mixer::save_scene(&store, &format!("scene-{index}")).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(mixer::load(&store).unwrap().scenes.len(), 8);
}
