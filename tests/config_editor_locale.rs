//! Each integration-test process has its own locale state; no parallel locale mutation in this file.
use crossterm::event::KeyCode;
use maris::{
    configuration::{Context, Editor},
    control_panel::{Overlay, Workspace},
    device_profile, i18n, listening,
    store::Store,
    tui_view::{self, Console},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

#[test]
fn six_languages_cover_pending_current_draft_apply_cancel_and_compact_notice() {
    let now = maris::analysis::now_ms();
    let _clock = maris::analysis::DebugClock::freeze_at(now);
    for locale in i18n::LANGUAGES {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path());
        i18n::configure(&store, Some(locale)).unwrap();
        let runtime = json!({"active":true,"session_id":"offline-locale",
            "output":"Test Headphones","profile_key":"Test Headphones","sample_rate":48000,
            "updated_at_ms":maris::analysis::now_ms(),
            "device_capability":device_profile::effective(&store,"Test Headphones").unwrap()});
        store.write_json("runtime.json", &runtime).unwrap();
        let eq = store.load().unwrap();
        let library = listening::load(&store).unwrap();
        let mut editor = Editor::default();
        let mut row = 16;
        editor
            .handle(
                &store,
                Context {
                    runtime: &runtime,
                    eq: &eq,
                    listening: &library,
                },
                &mut row,
                KeyCode::Char('+'),
            )
            .unwrap();
        let models = maris::neural::models();
        let apps = json!({});
        let view = Console {
            snapshot: &eq,
            configuration: Some(&editor),
            runtime: &runtime,
            workspace: Workspace::Sound,
            sound_row: row,
            home_row: 0,
            sound_scroll: 0,
            app_row: 0,
            pending_apps: &[],
            devices: &[],
            applications: &apps,
            selected_output: None,
            output_choice: 0,
            presets: &[],
            preset_choice: 0,
            models: &models,
            music: &library.default,
            palette: maris::theme::VIOLET,
            notice: "OFFLINE FIXTURE",
            goal: "balanced",
            proposal: None,
            overlay: Overlay::None,
        };
        for (width, height) in [(100, 32), (140, 40), (180, 50), (80, 24)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
            // Continuation cells belong to a wide CJK glyph, not a visible extra space.
            let mut text = String::new();
            for cells in terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
            {
                let mut column = 0;
                while column < cells.len() {
                    let symbol = cells[column].symbol();
                    text.push_str(symbol);
                    column += ratatui::text::Line::from(symbol).width().max(1);
                }
                text.push('\n');
            }
            let keys: &[&str] = if width < 90 {
                &["Draft pending · enlarge to apply · Esc cancels"]
            } else {
                &[
                    "Sound settings",
                    "Current",
                    "Draft",
                    "Pending changes",
                    "Enter Apply changes",
                    "Esc Cancel draft",
                ]
            };
            for key in keys {
                assert!(i18n::has_translation(key), "unregistered: {key}");
                assert!(
                    text.contains(i18n::for_language(locale, key)),
                    "{locale} {width}x{height}: missing {key}"
                );
            }
            assert!(text.contains("OFFLINE FIXTURE"));
        }
        assert_eq!(store.load().unwrap().revision, 0);
        assert!(!directory.path().join("profile.json").exists());
        assert!(!directory.path().join("listening.json").exists());
        assert!(!directory.path().join("control.json").exists());
    }
}
