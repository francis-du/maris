//! Seeded offline release checks. Each external runner invocation selects a recorded seed.
//! These cases do not capture audio, touch OS routing, or establish subjective quality.
#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use crossterm::event::KeyCode;
use maris::{
    configuration::{Context, Editor, Outcome},
    control_panel::UndoTarget,
    device_profile::Capability,
    dsp::{Processor, Settings},
    listening,
    music::MusicProfile,
    profile::Profile,
    store::Store,
};
use serde_json::json;

#[test]
fn seeded_release_safety_case() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let seed: u64 = std::env::var("MARIS_CHECK_SEED")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    assert!((1..=10000).contains(&seed));
    let rate = [44100, 48000, 96000, 192000][seed as usize % 4];
    let catalog = maris::presets::list();
    let preset = &catalog[seed as usize % catalog.len()];
    let eq = maris::presets::profile(&preset.id, rate).unwrap();
    let scenes = [
        "focus",
        "long-listening",
        "dialogue",
        "cinema",
        "night-dialogue",
        "acoustic-listening",
        "orchestral",
        "tight-bass",
        "game-clarity",
        "small-speakers",
        "surround-360",
        "cinema-360",
        "stereo-focus",
    ];
    let scene = scenes[(seed as usize / 4) % scenes.len()];
    let before = MusicProfile {
        correction_preamp_db: -1.0,
        correction_source: Some("Offline fixture correction".into()),
        ..MusicProfile::default()
    };
    let preview = maris::scenes::prepare(&before, &Capability::default(), scene).unwrap();
    assert_eq!(preview.profile.correction_source, before.correction_source);
    assert_eq!(
        preview.profile.correction_preamp_db,
        before.correction_preamp_db
    );
    let mut dsp = Processor::new(
        Settings::compile(&eq, rate)
            .unwrap()
            .with_music(&preview.profile, rate)
            .unwrap(),
    );
    let mut random = seed;
    for index in 0..8192 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let x = ((random >> 32) as u32 as f64 / u32::MAX as f64 * 4.0 - 2.0) as f32;
        let frame = match index % 997 {
            0 => [f32::NAN, f32::INFINITY],
            1 => [f32::NEG_INFINITY, 0.0],
            _ => [x, if seed.is_multiple_of(2) { x } else { -x }],
        };
        let output = dsp.process(frame);
        assert!(output.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let initial = store.load().unwrap();
    let library = listening::load(&store).unwrap();
    let runtime = json!({"active":true,"session_id":"offline-round","output":"Fixture",
        "profile_key":"Fixture","sample_rate":rate,"music_processing":true,"rebind_count":0,
        "updated_at_ms":maris::analysis::now_ms(),"device_capability":Capability::default()});
    store.write_json("runtime.json", &runtime).unwrap();
    let mut editor = Editor::default();
    let mut row = 16 + seed as usize % 10;
    let baseline_row = row;
    for _ in 0..(seed % 4 + 1) {
        let runtime = live_telemetry::refresh(&store);
        editor
            .handle(
                &store,
                Context {
                    runtime: &runtime,
                    eq: &initial,
                    listening: &library,
                },
                &mut row,
                KeyCode::Char('+'),
            )
            .unwrap();
    }
    assert!(!store.directory.join("profile.json").exists());
    let runtime = live_telemetry::refresh(&store);
    let outcome = editor
        .handle(
            &store,
            Context {
                runtime: &runtime,
                eq: &initial,
                listening: &library,
            },
            &mut row,
            KeyCode::Enter,
        )
        .unwrap();
    assert_eq!(outcome, Outcome::Applied(Some(UndoTarget::Profile)));
    let current = store.load().unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(
        current.profile.bands[baseline_row - 16].gain_db,
        (seed % 4 + 1) as f64 * 0.5
    );
    assert_eq!(store.undo(Some(1)).unwrap().profile, Profile::default());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    println!(
        "{}",
        json!({"seed":seed,"sample_rate":rate,"eq":preset.id,"scene":scene,
                         "generated_frames":8192,"draft_transaction_verified":true,"hardware_validation":false})
    );
}
