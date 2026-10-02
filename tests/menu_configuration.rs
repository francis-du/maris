use maris::{
    control,
    desktop_controls::{Controller, Summary},
    i18n, listening, mcp,
    store::Store,
};
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    store
        .write_json(
            "runtime.json",
            &json!({"active":true,"music_processing":true,
        "session_id":"fixture","profile_key":"Headphones","output":"Headphones",
        "sample_rate":48000,"updated_at_ms":maris::analysis::now_ms(),
        "system_backend":"coreaudio_process_tap","applied_revision":0,"applied_music_revision":0}),
        )
        .unwrap();
    (directory, store)
}

#[test]
fn malformed_or_missing_output_selection_never_means_system_default() {
    for invalid in [
        json!({}),
        json!({"output":false}),
        json!({"output":0}),
        json!({"output":[]}),
        json!({"output":""}),
        json!({"output":"uid:a\n"}),
    ] {
        assert!(control::parse_output_selection(&invalid).is_err());
    }
    assert_eq!(
        control::parse_output_selection(&json!({"output":null})).unwrap(),
        None
    );
    assert_eq!(
        control::parse_output_selection(&json!({"output":"uid:a"})).unwrap(),
        Some("uid:a")
    );
}

#[test]
fn mcp_requires_explicit_output_even_when_routing_is_confirmed() {
    let (_directory, store) = fixture();
    assert!(mcp::invoke(
        &store,
        "maris_output_select",
        json!({"confirm_routing":true}),
        true
    )
    .is_err());
    assert!(!store.directory.join("control.json").exists());
}

#[test]
fn global_bypass_is_not_misrepresented_as_an_audible_ab_comparison() {
    let (_directory, store) = fixture();
    store
        .edit(Some(0), |profile| {
            profile.bypass = true;
            Ok(())
        })
        .unwrap();
    let mut controller = Controller::default();
    assert!(controller.compare(&store).is_err());
    assert!(controller.select_preset(&store, "focus").is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    let state = maris::audio::runtime_status(&store);
    let observed_at = state["updated_at_ms"].as_u64().unwrap();
    let summary = Summary::read(&store, &state, observed_at).unwrap();
    assert!(!summary.controls_enabled);
    assert!(
        summary.output_enabled,
        "bypass does not remove the native output backend"
    );
    assert_eq!(
        summary.compare,
        format!("A/B: {}", i18n::text("Processing bypassed"))
    );
}

#[test]
fn menu_control_strings_and_confirmation_templates_cover_all_six_languages() {
    for key in [
        "Listening presets",
        "Listening preset",
        "Apply selection",
        "Cancel selection",
        "Selection details",
        "Compare reference / enhanced",
        "Undo listening change",
        "Enable processing",
        "Bypass processing (keep safety gain)",
        "Processing bypassed",
        "Global bypass is active; enable processing first",
        "Apply: {selection}",
        "Selection ready; review then apply",
        "Configuration changed; reopen the picker",
        "No current audio telemetry",
        "Output must be a selector or explicit null",
        "System volume and default output stay unchanged",
    ] {
        assert!(i18n::has_translation(key), "missing {key}");
        for code in i18n::LANGUAGES {
            assert!(!i18n::for_language(code, key).is_empty());
        }
    }
    for code in i18n::LANGUAGES {
        let name = "Studio {value} headphones";
        let rendered = i18n::format_for(code, "Apply: {selection}", &[("selection", name)]);
        assert!(rendered.contains(name));
        assert!(!i18n::language_name(code).is_empty());
    }
}
