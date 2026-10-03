#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use maris::{
    analysis,
    device_profile::Capability,
    listening,
    music::MusicProfile,
    music_context::MusicContext,
    music_tuning::{self, TuningInput},
    store::Store,
};

fn tone(hz: f64) -> Vec<[f32; 2]> {
    (0..96_000)
        .map(|index| {
            let sample = (std::f64::consts::TAU * hz * index as f64 / 48_000.0).sin() as f32 * 0.1;
            [sample, sample]
        })
        .collect()
}

fn mixed_tone(main_hz: f64, bright_hz: f64, bright_gain: f32) -> Vec<[f32; 2]> {
    (0..96_000)
        .map(|index| {
            let main =
                (std::f64::consts::TAU * main_hz * index as f64 / 48_000.0).sin() as f32 * 0.1;
            let bright = (std::f64::consts::TAU * bright_hz * index as f64 / 48_000.0).sin() as f32
                * 0.1
                * bright_gain;
            [main + bright, main + bright]
        })
        .collect()
}

fn semantic_context(evidence: &analysis::Analysis) -> MusicContext {
    let mut context = MusicContext::signal_only(evidence);
    context.mode = "semantic".into();
    context.model = "fixture-semantic".into();
    context.confidence = 0.8;
    context
}

fn live_runtime(
    store: &Store,
    device: &str,
    evidence: &analysis::Analysis,
    context: &MusicContext,
    now: u64,
) {
    store
        .write_json(
            "runtime.json",
            &serde_json::json!({
                "active":true,
                "session_id":"tuning-live-session",
                "output":device,
                "profile_key":device,
                "sample_rate":evidence.sample_rate,
                "updated_at_ms":now,
                "effective_preamp_db":-3.0,
                "device_identity":{"stable_id":"tuning-device-A"},
                "device_binding_revision":1,
                "rebind_count":0,
                "analysis":evidence,
                "music_context":context,
                "music_context_status":{"semantic_backend_available":context.mode == "semantic"}
            }),
        )
        .unwrap();
}

#[test]
fn bass_heavy_source_never_gets_more_static_bass() {
    let evidence = analysis::measure(&tone(100.0), 48_000).unwrap();
    let context = MusicContext::signal_only(&evidence);
    let before = MusicProfile {
        bass_db: 2.0,
        ..MusicProfile::default()
    };
    let capability = Capability {
        device_class: "built_in_compact_speaker".into(),
        max_preference_boost_db: 1.5,
        virtual_bass_allowed: true,
        ..Capability::default()
    };
    let proposal = music_tuning::propose_from(TuningInput {
        device: "MacBook Pro Speakers",
        profile_key: "MacBook Pro Speakers",
        listening_revision: 3,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "warm",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert_eq!(proposal.profile.bass_db, 1.5);
    assert_eq!(before.bass_db - proposal.profile.bass_db, 0.5);
    assert!(proposal.profile.adaptive.enabled);
    assert!(proposal.profile.adaptive.strength >= before.adaptive.strength);
}

#[test]
fn unknown_device_limits_block_speculative_bass_boost() {
    let evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let context = MusicContext::signal_only(&evidence);
    let before = MusicProfile::default();
    let capability = Capability::default();
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Unknown Output",
        profile_key: "Unknown Output",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "warm",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert_eq!(proposal.profile.bass_db, 0.0);
    assert!(!proposal.profile.bass_assist.enabled);
    assert!(proposal
        .reasons
        .iter()
        .any(|reason| reason.contains("withheld")));
}

#[test]
fn known_small_speaker_prefers_virtual_bass_over_sub_bass_boost() {
    let evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let context = MusicContext::signal_only(&evidence);
    let before = MusicProfile::default();
    let capability = Capability {
        device_class: "built_in_compact_speaker".into(),
        virtual_bass_allowed: true,
        bass_floor_hz: Some(90.0),
        max_preference_boost_db: 1.5,
        ..Capability::default()
    };
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Measured Compact Speaker",
        profile_key: "Measured Compact Speaker",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "warm",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert_eq!(proposal.profile.bass_db, 0.0);
    assert!(proposal.profile.bass_assist.enabled);
    assert!(proposal.profile.bass_assist.amount > before.bass_assist.amount);
}

#[test]
fn vocal_probability_contributes_to_presence_decision_without_inventing_genre() {
    let evidence = analysis::measure(&tone(300.0), 48_000).unwrap();
    let mut context = semantic_context(&evidence);
    context.vocal_probability = Some(0.9);
    let before = MusicProfile::default();
    let capability = Capability::default();
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Headphones",
        profile_key: "Headphones",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "balanced",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert!(proposal.profile.presence_db > before.presence_db);
    assert!(proposal
        .reasons
        .iter()
        .any(|reason| reason.contains("vocal semantic context")));
}

#[test]
fn semantic_mood_only_changes_softness_when_signal_supports_it() {
    let evidence = analysis::measure(&mixed_tone(1000.0, 8000.0, 0.35), 48_000).unwrap();
    let treble = evidence.band_energy_share[7..].iter().sum::<f64>();
    assert!(treble > 0.10, "fixture must contain measurable treble");
    assert!(
        treble < 0.16,
        "fixture must stay below signal-only harsh threshold"
    );
    let before = MusicProfile::default();
    let capability = Capability::default();
    let baseline_context = MusicContext::signal_only(&evidence);
    let baseline = music_tuning::propose_from(TuningInput {
        device: "Headphones",
        profile_key: "Headphones",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &baseline_context,
        capability: &capability,
        goal: "balanced",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    let mut context = semantic_context(&evidence);
    context.mood.push(maris::music_context::Tag {
        label: "mellow".into(),
        confidence: 0.8,
    });
    let semantic = music_tuning::propose_from(TuningInput {
        device: "Headphones",
        profile_key: "Headphones",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "balanced",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert_eq!(baseline.profile.softness, before.softness);
    assert!(semantic.profile.softness > baseline.profile.softness);
}

#[test]
fn context_tuning_never_changes_measurement_correction() {
    let evidence = analysis::measure(&tone(8000.0), 48_000).unwrap();
    let context = MusicContext::signal_only(&evidence);
    let mut before = MusicProfile {
        correction_source: Some("Measured fixture".into()),
        correction_preamp_db: -4.0,
        ..MusicProfile::default()
    };
    before.correction.push(maris::tone::Filter {
        kind: maris::tone::Kind::Peak,
        frequency_hz: 100.0,
        gain_db: 5.0,
        q: 1.0,
    });
    let capability = Capability {
        device_class: "headphone".into(),
        max_preference_boost_db: 3.0,
        ..Capability::default()
    };
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Headphone",
        profile_key: "Headphone",
        listening_revision: 1,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "soft",
        effective_preamp_db: -5.0,
    })
    .unwrap();
    assert_eq!(proposal.profile.correction, before.correction);
    assert_eq!(
        proposal.profile.correction_preamp_db,
        before.correction_preamp_db
    );
    assert_eq!(proposal.profile.correction_source, before.correction_source);
}

#[test]
fn apply_rejects_tampering_and_accepts_matching_live_state() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = analysis::now_ms();
    evidence.updated_at_ms = now;
    let context = MusicContext::signal_only(&evidence);
    live_runtime(&store, "Unknown Output", &evidence, &context, now);
    let proposal = music_tuning::from_live_at(&store, "soft", now).unwrap();

    let mut tampered = proposal.clone();
    tampered.profile.bass_db = 6.0;
    assert!(music_tuning::apply(&store, &tampered).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);

    live_telemetry::refresh(&store);
    let applied = music_tuning::apply(&store, &proposal).unwrap();
    assert_eq!(applied.revision, 1);
    assert_eq!(applied.effective("Unknown Output"), &proposal.profile);
}

#[test]
fn live_apply_rejects_session_and_device_rebind_but_accepts_fresh_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = analysis::now_ms();
    evidence.updated_at_ms = now;
    let context = MusicContext::signal_only(&evidence);
    live_runtime(&store, "Headphones", &evidence, &context, now);
    let proposal = music_tuning::from_live_at(&store, "soft", now).unwrap();
    let original: serde_json::Value =
        maris::store::read_json(&store.directory.join("runtime.json")).unwrap();

    for (field, value) in [
        ("session_id", serde_json::json!("new-session")),
        ("device_binding_revision", serde_json::json!(2)),
        ("rebind_count", serde_json::json!(1)),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        store.write_json("runtime.json", &changed).unwrap();
        assert!(
            music_tuning::apply(&store, &proposal).is_err(),
            "accepted changed {field}"
        );
    }
    let mut changed = original.clone();
    changed["device_identity"]["stable_id"] = serde_json::json!("tuning-device-B");
    store.write_json("runtime.json", &changed).unwrap();
    assert!(music_tuning::apply(&store, &proposal).is_err());

    let mut changed = original;
    changed["analysis"]["peak_dbfs"] = serde_json::json!(-9.0);
    changed["analysis"]["updated_at_ms"] = serde_json::json!(analysis::now_ms());
    changed["updated_at_ms"] = serde_json::json!(analysis::now_ms());
    store.write_json("runtime.json", &changed).unwrap();
    assert!(music_tuning::apply(&store, &proposal).is_ok());
}

#[test]
fn apply_rejects_output_change_after_preview() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = analysis::now_ms();
    evidence.updated_at_ms = now;
    let context = MusicContext::signal_only(&evidence);
    live_runtime(&store, "Headphones", &evidence, &context, now);
    let proposal = music_tuning::from_live_at(&store, "balanced", now).unwrap();
    let mut runtime: serde_json::Value =
        maris::store::read_json(&store.directory.join("runtime.json")).unwrap();
    runtime["output"] = serde_json::json!("MacBook Pro Speakers");
    store.write_json("runtime.json", &runtime).unwrap();
    assert!(music_tuning::apply(&store, &proposal).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

#[test]
fn live_preview_rejects_future_stale_wrong_rate_and_incomplete_evidence() {
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = 50_000;
    evidence.updated_at_ms = now;
    let baseline = serde_json::json!({"active":true,"updated_at_ms":now,"sample_rate":48000,"analysis":evidence});
    music_tuning::preview_evidence(&baseline, now).unwrap();
    for (field, value) in [
        ("updated_at_ms", now + 1),
        ("updated_at_ms", now - 6001),
        ("sample_rate", 44100),
        ("dropped_frames", 1),
    ] {
        let mut invalid = baseline.clone();
        invalid["analysis"][field] = serde_json::json!(value);
        assert!(
            music_tuning::preview_evidence(&invalid, now).is_err(),
            "accepted invalid {field}"
        );
    }
    let mut future_runtime = baseline.clone();
    future_runtime["updated_at_ms"] = serde_json::json!(now + 1);
    assert!(music_tuning::preview_evidence(&future_runtime, now).is_err());
    let mut stopped = baseline;
    stopped["active"] = serde_json::json!(false);
    assert!(music_tuning::preview_evidence(&stopped, now).is_err());
}

#[test]
fn cached_or_stale_semantic_objects_cannot_drive_live_tuning() {
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = 50_000;
    evidence.updated_at_ms = now;
    let mut context = semantic_context(&evidence);
    context.vocal_probability = Some(0.9);
    let mut runtime = serde_json::json!({"music_context":context,"music_context_status":{"semantic_backend_available":false},"models":{"downloaded":true}});
    assert_eq!(
        music_tuning::live_context(&runtime, &evidence, now).mode,
        "signal_only"
    );
    runtime["music_context_status"]["semantic_backend_available"] = serde_json::json!(true);
    assert_eq!(
        music_tuning::live_context(&runtime, &evidence, now).mode,
        "semantic"
    );
    for time in [1, now + 1] {
        runtime["music_context"]["updated_at_ms"] = serde_json::json!(time);
        assert_eq!(
            music_tuning::live_context(&runtime, &evidence, now).mode,
            "signal_only"
        );
    }
}

#[test]
fn final_delta_reports_device_caps_and_boolean_changes_exactly() {
    let evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let context = MusicContext::signal_only(&evidence);
    let mut before = MusicProfile {
        bass_db: 4.0,
        ..MusicProfile::default()
    };
    before.bass_assist.enabled = true;
    let capability = Capability {
        max_preference_boost_db: 1.0,
        virtual_bass_allowed: false,
        ..Capability::default()
    };
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Fixture",
        profile_key: "Fixture",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "balanced",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert_eq!(proposal.profile.bass_db, 1.0);
    assert!(proposal
        .changes
        .iter()
        .any(|change| change == "Bass: +4.00 dB -> +1.00 dB"));
    assert!(proposal
        .changes
        .iter()
        .any(|change| change == "Bass Assist: ON -> OFF"));
    assert_eq!(
        proposal.changes,
        music_tuning::describe_changes(&before, &proposal.profile)
    );
}

#[test]
fn soft_goal_does_not_silently_add_compression_to_low_crest_audio() {
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    evidence.momentary_lufs = Some(-10.0);
    let context = MusicContext::signal_only(&evidence);
    let before = MusicProfile::default();
    let capability = Capability::default();
    let proposal = music_tuning::propose_from(TuningInput {
        device: "Fixture",
        profile_key: "Fixture",
        listening_revision: 0,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal: "soft",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    assert!(!proposal.profile.compressor.enabled);
    assert!(proposal
        .reasons
        .iter()
        .any(|reason| reason.contains("explicit compression choice")));
}

#[test]
fn no_op_preview_preserves_revision_and_the_previous_undo_snapshot() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    listening::edit(&store, Some(0), Some("Fixture"), |p| {
        p.bass_db = 1.0;
        Ok(())
    })
    .unwrap();
    let library = listening::edit(&store, Some(1), Some("Fixture"), |p| {
        p.bass_db = 0.0;
        Ok(())
    })
    .unwrap();
    let mut evidence = analysis::measure(&tone(1000.0), 48_000).unwrap();
    let now = analysis::now_ms();
    evidence.updated_at_ms = now;
    let context = MusicContext::signal_only(&evidence);
    let before = library.effective("Fixture");
    live_runtime(&store, "Fixture", &evidence, &context, now);
    let proposal = music_tuning::from_live_at(&store, "balanced", now).unwrap();
    assert_eq!(&proposal.profile, before);
    assert!(proposal.changes.is_empty());
    let mut tampered = proposal.clone();
    tampered.changes.push("Invented improvement".into());
    assert!(music_tuning::apply(&store, &tampered).is_err());
    let mut future = proposal.clone();
    future.created_at_ms = analysis::now_ms() + 60_000;
    assert!(music_tuning::apply(&store, &future).is_err());
    live_telemetry::refresh(&store);
    assert_eq!(music_tuning::apply(&store, &proposal).unwrap().revision, 2);
    assert_eq!(
        listening::undo(&store, Some(2))
            .unwrap()
            .effective("Fixture")
            .bass_db,
        1.0
    );
}

#[test]
fn listening_undo_is_monotonic_and_restores_device_preference() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let first = listening::edit(&store, Some(0), Some("Headphones"), |profile| {
        profile.bass_db = 1.0;
        Ok(())
    })
    .unwrap();
    assert_eq!(first.revision, 1);
    let second = listening::edit(&store, Some(1), Some("Headphones"), |profile| {
        profile.bass_db = 2.0;
        Ok(())
    })
    .unwrap();
    assert_eq!(second.revision, 2);
    let restored = listening::undo(&store, Some(2)).unwrap();
    assert_eq!(restored.revision, 3);
    assert_eq!(restored.effective("Headphones").bass_db, 1.0);
}
