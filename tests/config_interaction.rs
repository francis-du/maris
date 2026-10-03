//! Configuration-page regressions: browsing must not move targets or change playback policy.
#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use crossterm::event::KeyCode;
use maris::{
    configuration::{self, Context, Editor},
    device_profile::Capability,
    listening,
    store::Store,
    studio_controls,
};
use ratatui::layout::Rect;
use serde_json::json;

#[test]
fn clicking_a_visible_browser_row_keeps_every_visible_target_in_place() {
    for (width, height) in [(90, 26), (100, 32), (140, 40), (180, 50)] {
        let content = studio_controls::shell(Rect::new(0, 0, width, height)).content;
        let browser = configuration::layout(content).browser;
        for selected in configuration::ROW_ORDER {
            let original = configuration::browser_rows(browser, selected);
            for (clicked, _) in &original {
                assert_eq!(
                    configuration::browser_rows(browser, *clicked),
                    original,
                    "{width}x{height}: clicking {clicked} while {selected} is selected moves rows under the pointer"
                );
            }
        }
    }
}

#[test]
fn section_selection_is_one_action_and_arrows_cannot_enter_playback_switches() {
    use configuration::{Group, Outcome};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let eq = store.load().unwrap();
    let listening = listening::load(&store).unwrap();
    let runtime = json!({"active":false});
    let context = Context {
        runtime: &runtime,
        eq: &eq,
        listening: &listening,
    };
    let mut editor = Editor::default();
    let mut selected = 16;
    let mut all_rows = std::collections::BTreeSet::new();
    for group in Group::ALL {
        assert_eq!(
            editor
                .handle(&store, context, &mut selected, group.key())
                .unwrap(),
            Outcome::Browse
        );
        assert_eq!(selected, group.rows()[0]);
        for _ in 0..50 {
            all_rows.insert(selected);
            editor
                .handle(&store, context, &mut selected, KeyCode::Down)
                .unwrap();
            assert_eq!(Group::of(selected), group);
        }
        assert_eq!(selected, *group.rows().last().unwrap());
    }
    assert_eq!(all_rows.len(), 28);
    assert!(!editor.pending());
    assert_eq!(
        editor
            .handle(&store, context, &mut selected, KeyCode::Char('b'))
            .unwrap(),
        Outcome::Browse
    );
    assert!(!store.directory.join("profile.json").exists());
    assert!(!store.directory.join("listening.json").exists());
    assert!(!store.directory.join("control.json").exists());
}

#[test]
fn configuration_warning_text_is_available_in_every_supported_language() {
    for key in [
        "Device sound",
        "Playback switches",
        "Select a parameter",
        "Global bypass",
        "Bypass affects all outputs; safety gain and limiter remain.",
        "This switch also enables or disables this device's correction.",
        "A/B matching only attenuates; turning it off can change comparison level.",
        "Compression changes dynamics; no automatic makeup gain.",
        "Playback stays as selected; editing tone does not enable it.",
        "Edit here; the list only selects. Apply is separate below.",
        "Playback switches are separate from tone. Choose OFF or ON, review the scope, then apply.",
    ] {
        assert!(maris::i18n::has_translation(key), "missing {key}");
        for language in &maris::i18n::LANGUAGES[1..] {
            assert_ne!(
                maris::i18n::for_language(language, key),
                key,
                "{language}: {key}"
            );
        }
    }
}

#[test]
fn adjusting_a_draft_never_implicitly_enables_processing_or_exits_reference() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let library = listening::edit(&store, Some(0), Some("Headphones"), |p| {
        p.enabled = false;
        p.reference = true;
        Ok(())
    })
    .unwrap();
    let eq = store.load().unwrap();
    let runtime = json!({
        "active":true,"session_id":"configuration-fixture","profile_key":"Headphones",
        "output":"Headphones","sample_rate":48000,"music_processing":true,
        "device_identity":{"stable_id":"fixture"},"rebind_count":0,
        "device_capability":Capability::default(),"updated_at_ms":maris::analysis::now_ms()
    });
    store.write_json("runtime.json", &runtime).unwrap();
    let runtime = live_telemetry::refresh(&store);
    let context = Context {
        runtime: &runtime,
        eq: &eq,
        listening: &library,
    };
    let mut editor = Editor::default();
    let mut selected = 0;
    editor
        .handle(&store, context, &mut selected, KeyCode::Char('+'))
        .unwrap();
    assert_eq!(listening::load(&store).unwrap().revision, 1);
    let runtime = live_telemetry::refresh(&store);
    let context = Context {
        runtime: &runtime,
        eq: &eq,
        listening: &library,
    };
    editor
        .handle(&store, context, &mut selected, KeyCode::Enter)
        .unwrap();
    let updated = listening::load(&store).unwrap();
    assert_eq!(updated.effective("Headphones").bass_db, 0.5);
    assert!(!updated.effective("Headphones").enabled);
    assert!(updated.effective("Headphones").reference);
}
