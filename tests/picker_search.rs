//! Read-only picker filtering and rendered/pointer agreement; no audio devices.
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use maris::{
    audio::DeviceInfo,
    control_panel::{Overlay, Workspace},
    tui_view::{self, Console},
    ui::tui::{output_picker, picker_search::Search, preset_picker},
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use serde_json::json;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn search(query: &str) -> Search {
    let mut search = Search::default();
    assert!(search.text_key(key(KeyCode::Char('/'))));
    for ch in query.chars() {
        assert!(search.text_key(key(KeyCode::Char(ch))));
    }
    search
}
fn devices() -> Vec<DeviceInfo> {
    [
        "Built-in",
        "Studio Headphones",
        "Studio Headphones",
        "日本語 🎧",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| DeviceInfo {
        id: format!("uid:device-{index}"),
        name: name.into(),
        direction: "output".into(),
        is_default: index == 0,
    })
    .collect()
}
fn click(rect: Rect) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn filtering_preserves_stable_identity_even_with_duplicate_names() {
    let devices = devices();
    assert_eq!(search("studio").outputs(&devices), vec![2, 3]);
    assert_eq!(search("HEADPHONES device-2").outputs(&devices), vec![3]);
    assert_eq!(search("日本語").outputs(&devices), vec![4]);
    assert!(search("no-such-endpoint").outputs(&devices).is_empty());
    assert_eq!(search("automatic").outputs(&devices), vec![0]);
    assert_eq!(Search::default().outputs(&devices), vec![0, 1, 2, 3, 4]);
}

#[test]
fn query_is_bounded_control_free_and_backspace_removes_whole_graphemes() {
    let mut search = search("👩‍💻e\u{301}");
    assert!(search.text_key(key(KeyCode::Backspace)));
    assert_eq!(search.query(), "👩‍💻");
    assert!(search.text_key(key(KeyCode::Backspace)));
    assert_eq!(search.query(), "");
    for _ in 0..200 {
        search.text_key(key(KeyCode::Char('界')));
    }
    assert_eq!(search.query().len(), 255);
    let before = search.query().to_owned();
    search.text_key(key(KeyCode::Char('\n')));
    assert_eq!(search.query(), before);
    search.text_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(search.query().is_empty());
}

#[test]
fn text_preserves_case_and_repeated_letters_without_consuming_confirmation() {
    let mut search = search("Bass");
    let mut repeat = key(KeyCode::Char('s'));
    repeat.kind = KeyEventKind::Repeat;
    assert!(search.text_key(repeat));
    assert_eq!(search.query(), "Basss");
    assert!(!search.text_key(key(KeyCode::Enter)));
    let mut release = key(KeyCode::Char('q'));
    release.kind = KeyEventKind::Release;
    assert!(!search.text_key(release));
    assert!(!search.text_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT)));
    assert_eq!(search.query(), "Basss");
    assert!(search.text_key(key(KeyCode::Esc)));
    assert!(search.query().is_empty());
    assert!(!search.text_key(key(KeyCode::Esc)));
}

#[test]
fn selection_stays_on_a_visible_identity_and_empty_results_have_no_fallback() {
    let mut selected = 9;
    Search::reconcile(&[3, 9, 20], &mut selected);
    assert_eq!(selected, 9);
    Search::reconcile(&[3, 20], &mut selected);
    assert_eq!(selected, 3);
    Search::reconcile(&[], &mut selected);
    assert_eq!(selected, 3);
    Search::browse(KeyCode::End, &[], &mut selected, 5);
    assert_eq!(selected, 3);
}

#[test]
fn page_navigation_is_bounded_and_uses_catalog_indices_not_row_numbers() {
    let choices = [2, 7, 13, 40, 99];
    let mut selected = 7;
    for (key, expected) in [
        (KeyCode::PageDown, 40),
        (KeyCode::PageDown, 99),
        (KeyCode::PageUp, 13),
        (KeyCode::Home, 2),
        (KeyCode::Up, 2),
        (KeyCode::End, 99),
    ] {
        assert!(Search::browse(key, &choices, &mut selected, 2));
        assert_eq!(selected, expected);
    }
    assert!(!Search::browse(KeyCode::Enter, &choices, &mut selected, 2));
}

#[test]
fn output_pointer_maps_filtered_rows_to_original_identity_and_respects_guards() {
    let devices = devices();
    let search = search("studio");
    for (width, height) in [(30, 8), (60, 15), (140, 40)] {
        let area = Rect::new(0, 0, width, height);
        let layout = output_picker::layout(area, 2, 0);
        assert_eq!(
            output_picker::pointer_choice_filtered(
                area,
                &devices,
                2,
                false,
                &search,
                click(layout.rows)
            ),
            Some(2)
        );
        let mut next = click(layout.rows);
        next.row += 1;
        if layout.rows.height >= 2 {
            assert_eq!(
                output_picker::pointer_choice_filtered(area, &devices, 2, false, &search, next),
                Some(3)
            );
        }
        assert_eq!(
            output_picker::pointer_choice_filtered(area, &devices, 2, true, &search, next),
            None
        );
        assert_eq!(
            output_picker::pointer_choice_filtered(
                area,
                &devices,
                2,
                false,
                &search,
                click(layout.search)
            ),
            None
        );
    }
}

#[test]
fn search_labels_have_all_six_languages() {
    for key in ["Search", "/ Search", "No matches", "Esc clears search"] {
        assert!(maris::i18n::has_translation(key));
        for language in maris::i18n::LANGUAGES.iter().skip(1) {
            assert_ne!(maris::i18n::for_language(language, key), key);
        }
    }
}

#[test]
fn filtered_rendering_uses_one_geometry_and_hides_stale_previews() {
    let snapshot = maris::store::Snapshot::default();
    let runtime = json!({"active":false});
    let music = maris::music::MusicProfile::default();
    let devices = devices();
    let presets = maris::presets::console_catalog();
    for overlay in [Overlay::Output, Overlay::Preset] {
        for query in ["", "zzzz-no-match", "studio", "dialogue"] {
            let search = search(query);
            let choices = search.choices(overlay, &devices, &presets);
            let selected = choices.first().copied().unwrap_or(0);
            let view = Console {
                snapshot: &snapshot,
                configuration: None,
                runtime: &runtime,
                workspace: Workspace::Now,
                sound_row: 0,
                home_row: 0,
                sound_scroll: 0,
                app_row: 0,
                pending_apps: &[],
                devices: &devices,
                applications: &runtime,
                selected_output: None,
                output_choice: selected,
                presets: &presets,
                preset_choice: selected,
                models: &runtime,
                music: &music,
                palette: maris::theme::VIOLET,
                notice: "Offline search fixture",
                goal: "balanced",
                proposal: None,
                overlay,
            };
            for (width, height) in [(1, 1), (29, 7), (30, 8), (80, 24), (140, 8), (140, 40)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| tui_view::draw_with_search(frame, &view, &search))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
                if width == 140 {
                    assert!(text.contains("Search"));
                    if choices.is_empty() {
                        assert!(text.contains("No matches"), "{width}x{height}: {text}");
                        assert!(
                            !text.contains("Enter confirms"),
                            "empty results advertised confirmation"
                        );
                    } else if overlay == Overlay::Preset {
                        let layout = preset_picker::layout(buffer.area, choices.len(), 0);
                        if layout.rows.height == 0 {
                            assert!(text.contains("Enlarge terminal to choose"), "{text}");
                            assert!(!text.contains("Enter confirms"));
                            continue;
                        }
                        assert_eq!(
                            preset_picker::pointer_choice_filtered(
                                buffer.area,
                                &view,
                                &search,
                                click(layout.rows)
                            ),
                            Some(selected)
                        );
                    }
                }
            }
        }
    }
}
