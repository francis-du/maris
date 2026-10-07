use super::*;
use crate::{music::MusicProfile, profile::Profile};

fn settings() -> Settings {
    Settings::compile(&Profile::default(), 48_000).unwrap()
}

#[test]
fn startup_envelope_waits_for_actual_capture_frames_not_output_callbacks() {
    let metrics = Metrics::default();
    let queue = Arc::new(ArrayQueue::new(8192));
    let mut source = Source::Live(LiveSource::new(queue.clone(), 48_000));
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..48_000 {
        let (input, ready) = source.frame(&metrics);
        assert!(!ready);
        assert_eq!(render.frame(input, ready, false), [0.0; 2]);
    }
    assert!(!render.started);
    assert_eq!(render.gain, 0.0);
    for _ in 0..4096 {
        queue.push([0.5, -0.5]).unwrap();
    }
    let (input, ready) = source.frame(&metrics);
    assert!(ready);
    let first = render.frame(input, ready, false);
    assert!(render.started);
    assert!(render.gain < 0.001);
    assert!(first[0].abs() < 0.001);
    assert_eq!(first[0], -first[1]);
}

#[test]
fn queued_tonal_transition_is_identical_after_arbitrary_capture_startup_delay() {
    let profile = Profile::default();
    let music = MusicProfile::preset("detail").unwrap();
    let baseline = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music.output_switch_baseline(), 48_000)
        .unwrap();
    let target = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap()
        .with_transition_ms(48_000, 120);
    let (baseline, target) = baseline.share_transition_headroom(target);
    assert_eq!(baseline.effective_preamp_db(), target.effective_preamp_db());
    let mut direct = RenderState::new(baseline, 48_000);
    let mut delayed = RenderState::new(baseline, 48_000);
    direct.processor.update(target);
    delayed.processor.update(target);
    for _ in 0..24_000 {
        assert_eq!(delayed.frame([0.0; 2], false, false), [0.0; 2]);
    }
    for index in 0..12_000 {
        let x = (std::f32::consts::TAU * 8_000.0 * index as f32 / 48_000.0).sin() * 0.3;
        assert_eq!(
            direct.frame([x, -x], true, false),
            delayed.frame([x, -x], true, false)
        );
    }
}

#[test]
fn valid_digital_silence_starts_the_envelope_without_amplitude_gating() {
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..2400 {
        assert_eq!(render.frame([0.0; 2], true, false), [0.0; 2]);
    }
    assert!(render.started);
    assert_eq!(render.gain, 1.0);
}

#[test]
fn stop_fade_reaches_zero_monotonically_even_after_source_disappears() {
    let mut render = RenderState::new(settings(), 48_000);
    for _ in 0..2400 {
        render.frame([0.1; 2], true, false);
    }
    let mut previous = 1.0;
    for _ in 0..2400 {
        render.frame([0.1; 2], false, true);
        assert!(render.gain <= previous);
        previous = render.gain;
    }
    assert_eq!(render.gain, 0.0);
    assert_eq!(render.frame([1.0; 2], false, true), [0.0; 2]);
}

#[test]
fn expired_renderer_telemetry_still_rejects_a_staged_configuration() {
    use crate::{
        configuration::{Context, Editor, Outcome},
        device_profile, listening,
        store::Store,
    };
    use crossterm::event::KeyCode;
    use serde_json::json;
    let clock = crate::analysis::test_clock::Clock::freeze();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let key = "OFFLINE expiry fixture";
    let runtime = json!({"active":true,"session_id":"expiry-test","output":key,
        "profile_key":key,"sample_rate":48000,"music_processing":true,
        "device_capability":device_profile::effective(&store,key).unwrap(),
        "updated_at_ms":crate::analysis::now_ms()});
    store.write_json("runtime.json", &runtime).unwrap();
    let eq = store.load().unwrap();
    let library = listening::load(&store).unwrap();
    let context = Context {
        runtime: &runtime,
        eq: &eq,
        listening: &library,
    };
    let mut editor = Editor::default();
    let mut row = 0;
    assert_eq!(
        editor
            .handle(&store, context, &mut row, KeyCode::Char('+'))
            .unwrap(),
        Outcome::Staged
    );
    clock.advance(10_000);
    assert_eq!(crate::audio::runtime_status(&store)["stale"], true);
    assert!(editor
        .handle(&store, context, &mut row, KeyCode::Enter)
        .is_err());
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!directory.path().join("control.json").exists());
}
