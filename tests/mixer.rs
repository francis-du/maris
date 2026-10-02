use maris::{
    mixer::{self, Ducking, MixerConfig, MixerStrip, Processor},
    store::Store,
};

fn strip(id: &str, sends: [f64; 2]) -> MixerStrip {
    MixerStrip {
        id: id.into(),
        name: id.into(),
        sends,
        ..MixerStrip::default()
    }
}

#[test]
fn matrix_routes_multiple_strips_to_two_independent_buses() {
    let config = MixerConfig {
        strips: vec![strip("music", [1.0, 0.0]), strip("voice", [0.0, 1.0])],
        ..MixerConfig::default()
    };
    let mut mixer = Processor::new(config, 48000).unwrap();
    let out = mixer.process(&[[0.1, -0.1], [0.2, -0.2]]);
    assert!((out[0][0] - 0.1).abs() < 1e-6);
    assert!((out[0][1] + 0.1).abs() < 1e-6);
    assert!((out[1][0] - 0.2).abs() < 1e-6);
    assert!((out[1][1] + 0.2).abs() < 1e-6);
}

#[test]
fn mute_solo_pan_and_bus_gain_are_deterministic() {
    let mut left = strip("left", [1.0, 0.0]);
    left.pan = -1.0;
    left.solo = true;
    let mut ignored = strip("ignored", [1.0, 0.0]);
    ignored.gain_db = 12.0;
    let mut config = MixerConfig {
        strips: vec![left, ignored],
        ..MixerConfig::default()
    };
    config.buses[0].gain_db = -6.0;
    let mut mixer = Processor::new(config, 48000).unwrap();
    let out = mixer.process(&[[0.2, 0.2], [0.8, 0.8]]);
    assert!(out[0][0] > 0.13 && out[0][0] < 0.15);
    assert!(out[0][1].abs() < 1e-6);
}

#[test]
fn voice_trigger_ducks_only_marked_background_strips() {
    let mut music = strip("music", [1.0, 0.0]);
    music.duck_target = true;
    let mut voice = strip("voice", [1.0, 0.0]);
    voice.voice_trigger = true;
    voice.duck_target = false;
    let config = MixerConfig {
        strips: vec![music, voice],
        ducking: Ducking {
            enabled: true,
            threshold_dbfs: -40.0,
            attenuation_db: 12.0,
            attack_ms: 1.0,
            release_ms: 300.0,
        },
        ..MixerConfig::default()
    };
    let mut mixer = Processor::new(config, 48000).unwrap();
    let mut last = [[0.0; 2]; 2];
    for _ in 0..4800 {
        last = mixer.process(&[[0.2, 0.2], [0.2, 0.2]]);
    }
    assert!(last[0][0] < 0.3);
    assert!(last[0][0] > 0.2);
}

#[test]
fn mixer_state_supports_revision_undo_and_named_scenes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let state = mixer::edit(&store, Some(0), |config| {
        config.strips.push(strip("music", [1.0, 0.0]));
        Ok(())
    })
    .unwrap();
    assert_eq!(state.revision, 1);
    let saved = mixer::save_scene_checked(&store, Some(state.revision), "Listening").unwrap();
    assert_eq!(saved.revision, state.revision + 1);
    assert_eq!(saved.previous, state.previous);
    let state = mixer::edit(&store, Some(saved.revision), |config| {
        config.strips[0].mute = true;
        Ok(())
    })
    .unwrap();
    assert!(state.config.strips[0].mute);
    let restored = mixer::restore_scene(&store, "Listening").unwrap();
    assert!(!restored.config.strips[0].mute);
    let undone = mixer::undo(&store, Some(restored.revision)).unwrap();
    assert!(undone.config.strips[0].mute);
}

#[test]
fn capabilities_match_the_wired_mixer_data_plane() {
    let caps = mixer::capabilities();
    assert_eq!(caps["matrix_engine"], true);
    assert_eq!(caps["simultaneous_hardware_inputs"], true);
    assert_eq!(caps["simultaneous_hardware_outputs"], true);
    assert_eq!(caps["per_strip_eq"], true);
    assert_eq!(caps["per_strip_compression"], true);
    assert_eq!(caps["cross_device_clock_sync"], true);
    assert_eq!(caps["latency_compensation"], true);
    let application_audio = cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    ));
    assert_eq!(caps["selected_application_capture"], application_audio);
    assert_eq!(caps["selected_application_shared_route"], application_audio);
    assert_eq!(caps["independent_application_strips"], application_audio);
    assert_eq!(caps["application_volume"], application_audio);
    assert_eq!(caps["per_strip_neural_denoise"], cfg!(feature = "neural"));
}
