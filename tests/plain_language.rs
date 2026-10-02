//! User-facing wording is separate from protocol keys and user-provided names.
use maris::i18n;

#[test]
fn common_controls_explain_the_action_instead_of_internal_implementation() {
    for (key, english, chinese) in [
        ("Music context", "Music analysis", "音乐分析"),
        (
            "Waiting for evidence",
            "Waiting for audio analysis",
            "等待音频分析",
        ),
        ("AI tuning", "Tuning suggestions", "调音建议"),
        ("Intelligence Detail", "Tuning suggestions", "调音建议"),
        ("Intelligence", "Tuning suggestions", "调音建议"),
        ("PLANNER", "Tuning suggestions", "调音建议"),
        ("SMART LAB", "Tuning suggestions", "调音建议"),
        ("VOICE AI", "Speech noise reduction", "语音降噪"),
        ("Voice AI", "Speech noise reduction", "语音降噪"),
        ("local heuristic", "Local rules", "本机规则"),
        ("DROPOUTS", "Audio interruptions", "音频中断"),
        ("Hardware I/O", "Audio devices", "音频设备"),
        ("Mixer Matrix", "App mixer", "应用混音"),
        ("Semantic", "Music recognition", "音乐识别"),
        ("Underruns", "Audio buffer gaps", "音频缓冲不足"),
        ("Device identity", "Device details", "设备信息"),
        (
            "No current audio telemetry",
            "Audio status is unavailable; try again shortly",
            "暂时读不到音频状态，请稍后重试",
        ),
    ] {
        assert_eq!(i18n::for_language("en", key), english, "{key}");
        assert_eq!(i18n::for_language("zh-CN", key), chinese, "{key}");
    }
}

#[test]
fn shared_controls_describe_the_action_without_assuming_a_platform_or_ai_backend() {
    for language in i18n::LANGUAGES {
        let start = i18n::for_language(
            language,
            "It does not change system volume or the macOS default output.",
        );
        assert!(!start.contains("macOS"), "{language}: {start}");
        let shortcuts =
            i18n::for_language(language, "O output · P preset · J AI · S stop · Q close");
        assert!(
            !shortcuts
                .split(|c: char| !c.is_alphabetic())
                .any(|word| ["AI", "KI", "IA"].contains(&word)),
            "{language}: {shortcuts}"
        );
        assert_ne!(
            i18n::for_language(language, "Processing bypassed"),
            "Processing bypassed"
        );
    }
}

#[test]
fn readable_notices_do_not_rewrite_device_names_or_expand_arguments() {
    let name = "Music context {device} {count} 设备身份";
    for language in i18n::LANGUAGES {
        let message = i18n::Notice::new("Output request queued: {device}.").arg("device", name);
        assert!(message.for_language(language).contains(name));
        assert_eq!(i18n::preset_name("user-preset", name), name);
        assert!(i18n::diagnostic_for(language, "CoreAudio OSStatus -50")
            .contains("CoreAudio OSStatus -50"));
    }
    assert_eq!(
        i18n::for_language("en", "Unrecognized diagnostic"),
        "Unrecognized diagnostic"
    );
}

#[test]
fn every_language_preserves_each_templates_named_arguments() {
    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|tail| tail.split_once('}').map(|p| p.0))
            .collect()
    }
    for (key, _) in i18n::entries() {
        for language in i18n::LANGUAGES {
            assert_eq!(
                placeholders(key),
                placeholders(i18n::for_language(language, key)),
                "{language}: {key}"
            );
        }
    }
}
