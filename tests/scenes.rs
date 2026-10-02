use maris::{
    device_profile::Capability,
    dsp::{Processor, Settings},
    listening,
    music::MusicProfile,
    presets, scenes,
    sound_cli::{self, Action},
    store::Store,
    tone::{Filter, Kind},
};
use std::collections::BTreeSet;

fn corrected() -> MusicProfile {
    MusicProfile {
        correction: vec![Filter {
            kind: Kind::Peak,
            frequency_hz: 3000.0,
            gain_db: -2.0,
            q: 1.0,
        }],
        correction_preamp_db: -3.0,
        correction_source: Some("Fixture measurement".into()),
        highpass_hz: Some(45.0),
        balance: -0.1,
        level_match: false,
        ..MusicProfile::default()
    }
}

#[test]
fn scene_catalog_is_unique_localized_and_separate_from_legacy_eq() {
    let names: BTreeSet<_> = scenes::SCENES.iter().map(|scene| scene.id).collect();
    assert_eq!(names.len(), 10);
    assert_eq!(presets::list().len(), 27);
    let picker = presets::console_catalog();
    assert_eq!(picker.len(), 37);
    for preset in &picker {
        let key = maris::i18n::preset_key(&preset.id, &preset.name);
        assert!(
            maris::i18n::has_translation(key),
            "untranslated preset {}: {key}",
            preset.id
        );
    }
    assert!(picker[..10]
        .iter()
        .all(|p| p.category == "scene" && p.id.starts_with("scene:")));
    for scene in scenes::SCENES {
        assert!(maris::music::PRESETS.contains(&scene.id));
        assert!(maris::i18n::has_translation(scene.name), "{}", scene.name);
        assert!(
            maris::i18n::has_translation(scene.description),
            "{}",
            scene.id
        );
        let p = MusicProfile::preset(scene.id).unwrap();
        assert!(p.bass_db <= 1.5 && p.presence_db <= 1.5 && p.air_db <= 1.0);
        assert_eq!(p.width, 1.0);
    }
}

#[test]
fn every_scene_preserves_correction_balance_highpass_and_comparison_policy() {
    let before = corrected();
    let capability = Capability {
        max_preference_boost_db: 0.25,
        ..Capability::default()
    };
    for scene in scenes::SCENES {
        let preview = scenes::prepare(&before, &capability, scene.id).unwrap();
        assert_eq!(preview.profile.correction, before.correction);
        assert_eq!(preview.profile.correction_source, before.correction_source);
        assert_eq!(
            preview.profile.correction_preamp_db,
            before.correction_preamp_db
        );
        assert_eq!(preview.profile.highpass_hz, before.highpass_hz);
        assert_eq!(preview.profile.balance, before.balance);
        assert_eq!(preview.profile.level_match, before.level_match);
        assert!(preview.profile.bass_db <= 0.25);
        assert!(preview.profile.presence_db <= 0.25);
        assert!(preview.profile.air_db <= 0.25);
        assert_eq!(
            preview.changes,
            maris::music_tuning::describe_changes(&before, &preview.profile)
        );
    }
}

#[test]
fn small_speaker_scene_never_guesses_physical_limits_or_boosts_subbass() {
    let before = MusicProfile::default();
    for capability in [
        Capability::default(),
        Capability {
            device_class: "headphone".into(),
            ..Capability::default()
        },
        Capability {
            device_class: "speaker".into(),
            virtual_bass_allowed: true,
            ..Capability::default()
        },
    ] {
        let preview = scenes::prepare(&before, &capability, "small-speakers").unwrap();
        assert!(!preview.profile.bass_assist.enabled);
        assert!(preview.profile.bass_db <= 0.0);
    }
    let known = Capability {
        device_class: "speaker".into(),
        virtual_bass_allowed: true,
        bass_floor_hz: Some(90.0),
        ..Capability::default()
    };
    let preview = scenes::prepare(&before, &known, "small-speakers").unwrap();
    assert!(preview.profile.bass_assist.enabled);
    assert!(preview.profile.bass_assist.amount <= 0.2);
}

#[test]
fn only_explicit_night_dialogue_scene_enables_compression() {
    for scene in scenes::SCENES {
        let preview =
            scenes::prepare(&MusicProfile::default(), &Capability::default(), scene.id).unwrap();
        assert_eq!(preview.compression_enabled, scene.id == "night-dialogue");
    }
    assert!(MusicProfile::preset("night").unwrap().compressor.enabled);
}

#[test]
fn scene_preview_apply_and_undo_share_the_listening_transaction_only() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let before = corrected();
    listening::edit(&store, Some(0), Some("Studio Headphones"), |p| {
        *p = before.clone();
        Ok(())
    })
    .unwrap();
    let prior = std::fs::read(directory.path().join("listening.json")).unwrap();
    let preview = sound_cli::run(
        &store,
        Some("Studio Headphones".into()),
        None,
        Action::Preview {
            name: "night-dialogue".into(),
        },
    )
    .unwrap();
    assert_eq!(preview["applied"], false);
    assert_eq!(
        std::fs::read(directory.path().join("listening.json")).unwrap(),
        prior
    );
    assert!(!directory.path().join("control.json").exists());
    assert!(!directory.path().join("runtime.json").exists());
    assert!(
        listening::preset(&store, Some(0), Some("Studio Headphones"), "night-dialogue").is_err()
    );
    let applied = listening::preset(
        &store,
        Some(1),
        Some("Studio Headphones"),
        "scene:night-dialogue",
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(applied.effective("Studio Headphones")).unwrap(),
        preview["preview"]["profile"]
    );
    assert_eq!(store.load().unwrap().revision, 0);
    let undone = listening::undo(&store, Some(2)).unwrap();
    assert_eq!(undone.effective("Studio Headphones"), &before);
    assert_eq!(undone.revision, 3);
    assert_eq!(undone.effective("Other Output"), &MusicProfile::default());
}

#[test]
fn every_scene_compiles_and_stays_finite_peak_limited_at_supported_rates() {
    for rate in [44100, 48000, 96000, 192000] {
        for scene in scenes::SCENES {
            let preview = scenes::prepare(&corrected(), &Capability::default(), scene.id).unwrap();
            let settings = Settings::compile(&maris::profile::Profile::default(), rate)
                .unwrap()
                .with_music(&preview.profile, rate)
                .unwrap();
            let mut processor = Processor::new(settings);
            for i in 0..4096 {
                let x = if i == 17 {
                    f32::NAN
                } else {
                    (i as f32 * 0.31).sin() * 8.0
                };
                assert!(processor
                    .process([x, -x])
                    .into_iter()
                    .all(|s| s.is_finite() && s.abs() <= 0.891252));
            }
        }
    }
}

#[test]
fn unknown_scene_fails_before_creating_any_preference() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    assert!(listening::preset(&store, Some(0), None, "scene:missing").is_err());
    assert!(!dir.path().join("listening.json").exists());
}
