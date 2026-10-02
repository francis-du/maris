use maris::{
    audio::DeviceInfo,
    control_panel::{Overlay, Workspace},
    i18n,
    store::{Snapshot, Store},
    tui_view::{self, Console},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

fn visible_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for row in buffer.content.chunks(buffer.area.width as usize) {
        let mut column = 0;
        while column < row.len() {
            let symbol = row[column].symbol();
            text.push_str(symbol);
            column += ratatui::text::Line::from(symbol).width().max(1);
        }
        text.push('\n');
    }
    text
}

fn render(view: &Console<'_>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| tui_view::draw(frame, view)).unwrap();
    visible_text(&terminal)
}

// One test owns locale mutation; independent locale tests use for_language instead.
#[test]
fn six_locales_cover_home_inspectors_overlays_notices_and_live_preference_changes() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let snapshot = Snapshot::default();
    let music = maris::music::MusicProfile::default();
    let original_profile = serde_json::to_string(&music).unwrap();
    let presets = maris::presets::console_catalog();
    let models = maris::neural::models();
    let devices = vec![DeviceInfo {
        id: "uid:fixture".into(),
        name: "Warm Studio Device".into(),
        direction: "output".into(),
        is_default: true,
    }];
    let applications = json!({"available":true,"applications":[
        {"pid":111,"bundle_id":"fixture.music","running_output":true,"is_maris":false},
        {"pid":222,"bundle_id":"fixture.browser","running_output":true,"is_maris":false},
        {"pid":333,"bundle_id":"fixture.idle","running_output":false,"is_maris":false}
    ]});
    let frames: Vec<_> = (0..96_000)
        .map(|index| {
            let value =
                (index as f64 * std::f64::consts::TAU * 997.0 / 48_000.0).sin() as f32 * 0.1;
            [value, value]
        })
        .collect();
    let mut evidence = maris::analysis::measure(&frames, 48_000).unwrap();
    let context = maris::music_context::MusicContext::signal_only(&evidence);
    let plan = maris::music_tuning::propose_from(maris::music_tuning::TuningInput {
        device: "Warm Studio Device",
        profile_key: "Warm Studio Device",
        listening_revision: 0,
        before: &music,
        evidence: &evidence,
        context: &context,
        capability: &maris::device_profile::Capability::default(),
        goal: "soft",
        effective_preamp_db: -3.0,
    })
    .unwrap();
    let original_plan = serde_json::to_string(&plan).unwrap();
    for language in i18n::LANGUAGES {
        i18n::configure(&store, Some(language)).unwrap();
        for workspace in Workspace::ALL {
            let now = maris::analysis::now_ms();
            evidence.updated_at_ms = now;
            let runtime = json!({
                "active":true,"updated_at_ms":now,"sample_rate":48000,
                "output":"Warm Studio Device","output_mode":"pinned","profile_key":"Warm Studio Device",
                "device_capability":{"device_class":"headphone"},
                "device_identity":{"stable_id":"fixture"},"profile_binding_source":"stable_id",
                "system_backend":"coreaudio_process_tap","capture_scope":"selected_processes",
                "captured_application_pids":[111],"analysis":evidence,
                "last_control_result":{"action":"select_output","ok":true},
                "requested_music_revision":0,"applied_music_revision":0,"applied_revision":0
            });
            let notice = i18n::Notice::new("AI goal: {goal}")
                .label_arg("goal", "Balanced")
                .render();
            let mut view = Console {
                snapshot: &snapshot,
                configuration: None,
                runtime: &runtime,
                workspace,
                sound_row: 16,
                home_row: 1,
                sound_scroll: 0,
                app_row: 0,
                pending_apps: &[111],
                devices: &devices,
                applications: &applications,
                selected_output: devices.first(),
                output_choice: 0,
                presets: &presets,
                preset_choice: 4,
                models: &models,
                music: &music,
                palette: maris::theme::VIOLET,
                notice: &notice,
                goal: "balanced",
                proposal: None,
                overlay: Overlay::None,
            };
            // Wide, medium, compact and tiny paths must all remain bounded and renderable.
            for (width, height) in [(40, 12), (80, 24), (100, 32), (140, 40), (220, 68)] {
                assert!(render(&view, width, height).contains("MARIS"));
            }
            let text = render(&view, 220, 68);
            assert!(
                text.contains("Warm Studio Device"),
                "device name changed in {language}"
            );
            if workspace == Workspace::Sound {
                // Each fixed section is one direct selection, rather than a mixed live list.
                for (selected, keys) in [
                    (0, &["Device sound", "Bass Assist", "Softness"][..]),
                    (16, &["Global EQ", "Current", "Draft"][..]),
                    (14, &["Playback switches", "Level match", "ON", "OFF"][..]),
                ] {
                    let section = Console {
                        sound_row: selected,
                        ..view
                    };
                    let rendered = render(&section, 220, 68);
                    for key in keys {
                        assert!(
                            rendered.contains(i18n::for_language(language, key)),
                            "missing {key}: {language} settings row {selected}"
                        );
                    }
                }
            } else {
                assert!(
                    text.contains(i18n::for_language(language, "Bass Assist")),
                    "{language} {workspace:?}"
                );
            }
            let keys: &[&str] = match workspace {
                Workspace::Now => &["Listening Assist", "Balanced", "Sound"],
                Workspace::Sound => &["Sound settings", "Global EQ", "Current", "Draft"],
                Workspace::Apps => &["Routed", "Playing", "Idle", "Pending selection"],
                Workspace::Device => &["Pinned output", "Headphones", "Device identity"],
                Workspace::Intelligence => {
                    &["Choose a goal and preview", "Ready for preview", "OFF"]
                }
                Workspace::System => &[
                    "Native process tap",
                    "Selected applications",
                    "Select output",
                ],
            };
            for key in keys {
                assert!(
                    text.contains(i18n::for_language(language, key)),
                    "missing {key}: {language} {workspace:?}"
                );
            }
            if workspace == Workspace::Now {
                for (overlay, key) in [
                    (Overlay::Output, "Follow system default"),
                    (Overlay::Preset, "Night dialogue"),
                    (Overlay::Help, "HELP · simple mode"),
                    (Overlay::StartSystem, "START SYSTEM AUDIO"),
                ] {
                    view.overlay = overlay;
                    let text = render(&view, 220, 68);
                    assert!(
                        text.contains(i18n::for_language(language, key)),
                        "{language} {overlay:?}"
                    );
                    assert!(
                        text.contains(i18n::for_language(language, "Enter confirms · Esc cancels"))
                    );
                    if overlay == Overlay::Preset {
                        assert!(text.contains(i18n::for_language(language, "Compression enabled")));
                        assert!(text
                            .contains(i18n::for_language(language, "Correction stays unchanged")));
                    }
                }
                view.overlay = Overlay::Proposal;
                view.proposal = Some(&plan);
                let text = render(&view, 220, 68);
                assert!(text.contains(i18n::for_language(language, "AI PREVIEW")));
                assert!(text.contains(i18n::for_language(language, "Soft")));
                assert!(text.contains(i18n::for_language(language, "Softness")));
                assert!(text.contains(i18n::for_language(language, "Enter applies · Esc cancels")));
                assert_eq!(serde_json::to_string(&plan).unwrap(), original_plan);
            }
        }
        assert_eq!(serde_json::to_string(&music).unwrap(), original_profile);
        assert_eq!(store.load().unwrap().revision, 0);
        assert!(!directory.path().join("listening.json").exists());
        let change = i18n::change("Compressor: OFF -> ON");
        assert!(change.contains(i18n::for_language(language, "Compressor")));
        assert!(change.contains(i18n::for_language(language, "ON")));
    }

    i18n::save(&store, "en").unwrap();
    let mut watcher = i18n::Watcher::new(&store);
    i18n::configure(&store, Some("ja")).unwrap();
    assert!(!watcher.refresh(&store));
    assert_eq!(
        i18n::code(),
        "ja",
        "unchanged saved value must not override explicit startup language"
    );
    store
        .write_json("ui.json", &json!({"language":"es"}))
        .unwrap();
    assert!(watcher.refresh(&store));
    assert_eq!(i18n::code(), "es");
    store
        .write_json("ui.json", &json!({"language":"invalid"}))
        .unwrap();
    assert!(!watcher.refresh(&store));
    assert_eq!(i18n::code(), "es");
    i18n::configure(&store, Some("en")).unwrap();
}
