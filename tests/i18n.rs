use maris::{i18n, music::MusicProfile, music_view, store::Store};
use ratatui::{backend::TestBackend, Terminal};
#[test]
fn locale_tags_and_english_fallback() {
    assert_eq!(i18n::normalize("zh_Hant_TW.UTF-8"), 2);
    assert_eq!(i18n::normalize("zh_CN.UTF-8"), 1);
    assert_eq!(i18n::normalize("ja_JP.UTF-8"), 3);
    assert_eq!(
        i18n::for_language("unknown", "Unknown diagnostic"),
        "Unknown diagnostic"
    );
    assert_eq!(i18n::for_language("zh-CN", "Bass"), "低音");
}
#[test]
fn every_localized_key_has_all_translations() {
    let mut keys = std::collections::BTreeSet::new();
    for (key, translations) in i18n::entries() {
        assert!(keys.insert(key), "duplicate translation: {key}");
        assert!(translations.iter().all(|s| !s.trim().is_empty()));
        for code in &i18n::LANGUAGES[1..] {
            assert!(!i18n::for_language(code, key).is_empty());
        }
    }
}
#[test]
fn every_control_surface_key_is_registered_for_localization() {
    for key in i18n::REQUIRED_CONTROL_KEYS {
        assert!(
            i18n::has_translation(key),
            "missing localization key: {key}"
        );
        for code in &i18n::LANGUAGES[1..] {
            assert!(!i18n::for_language(code, key).trim().is_empty());
        }
    }
}

#[test]
fn rewritten_tui_literal_keys_are_localized() {
    for source in [
        include_str!("../src/ui/tui/dashboard.rs"),
        include_str!("../src/ui/tui/settings/view.rs"),
        include_str!("../src/ui/tui/settings/mod.rs"),
        include_str!("../src/ui/tui/inspector.rs"),
        include_str!("../src/ui/tui/studio/mod.rs"),
        include_str!("../src/ui/tui/view.rs"),
        include_str!("../src/ui/tui/monitor.rs"),
    ] {
        for tail in source.split("t(\"").skip(1) {
            let Some(end) = tail.find("\")") else {
                continue;
            };
            let key = &tail[..end];
            assert!(i18n::has_translation(key), "missing TUI translation: {key}");
        }
    }
}

#[test]
fn templates_preserve_placeholders_and_do_not_expand_argument_text() {
    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|tail| tail.split_once('}').map(|p| p.0))
            .collect()
    }
    for (key, translations) in i18n::entries() {
        for translated in translations {
            assert_eq!(
                placeholders(key),
                placeholders(translated),
                "placeholder mismatch: {key}"
            );
        }
    }
    for code in i18n::LANGUAGES {
        let device = "My Headphones {count}";
        let output = i18n::format_for(
            code,
            "Output request queued: {device}.",
            &[("device", device), ("count", "17")],
        );
        assert!(output.contains(device));
        assert!(!output.contains("17"));
    }
}

#[test]
fn notices_relocalize_without_translating_device_names_or_wire_data() {
    let device = "Warm {goal} Headphones";
    let notice = i18n::Notice::new("Output request queued: {device}.").arg("device", device);
    for code in i18n::LANGUAGES {
        let rendered = notice.for_language(code);
        assert!(rendered.contains(device));
    }
    let goal = i18n::Notice::new("AI goal: {goal}").label_arg("goal", "Balanced");
    assert_eq!(goal.for_language("zh-CN"), "调音目标：均衡");
    assert_eq!(goal.for_language("en"), "Listening goal: Balanced");
    let known = i18n::diagnostic_for("zh-CN", "Analysis is warming up");
    assert_eq!(known, "正在收集音频分析数据");
    let raw = "CoreAudio OSStatus -50";
    assert!(i18n::diagnostic_for("zh-CN", raw).contains(raw));
}

#[test]
fn unsupported_preference_does_not_create_file() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    assert!(i18n::save(&store, "not-a-language").is_err());
    assert!(!temp.path().join("ui.json").exists());
}
#[test]
fn music_panel_handles_small_and_wide_terminals() {
    for (width, height) in [(40, 12), (64, 18), (80, 24), (120, 36)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                music_view::draw(frame, &MusicProfile::default(), "Headphones", 7, "Ready")
            })
            .unwrap();
    }
}
