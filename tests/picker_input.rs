//! Search editing and terminal-cell rendering; only synthetic input is used.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use maris::{control_panel::Overlay, ui::tui::picker_search::Search};
use ratatui::{backend::TestBackend, layout::Rect, text::Span, Terminal};

const AREA: Rect = Rect::new(0, 0, 140, 40);

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn search(text: &str) -> Search {
    let mut search = Search::default();
    search.text_key(key(KeyCode::Char('/')));
    for ch in text.chars() {
        search.text_key(key(KeyCode::Char(ch)));
    }
    search
}
fn rendered(search: &Search, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
    terminal
        .draw(|frame| search.draw(frame, frame.area(), 2, 46, maris::theme::VIOLET))
        .unwrap();
    let cells = &terminal.backend().buffer().content;
    let mut text = String::new();
    let mut index = 0;
    while index < cells.len() {
        let symbol = cells[index].symbol();
        text.push_str(symbol);
        // Continuation cells of a wide grapheme are not additional displayed spaces.
        index += Span::raw(symbol).width().max(1);
    }
    text
}

#[test]
fn long_search_keeps_the_edited_tail_cursor_and_result_count_visible() {
    for tail in ["last-device-42", "日本語🎧", "👩‍💻-end", "e\u{301}-tail"] {
        let search = search(&format!("{} {tail}", "long-prefix ".repeat(8)));
        let text = rendered(&search, 50);
        assert!(text.contains(tail), "edited tail disappeared: {text:?}");
        assert!(text.contains('▏'), "cursor disappeared: {text:?}");
        assert!(text.contains("2/46"), "result count disappeared: {text:?}");
        assert!(text.contains('…'));
    }
}

#[test]
fn pasted_line_breaks_are_search_separators_never_confirmations() {
    let mut search = search("");
    assert!(search.handle_input(
        &Event::Paste("Bass\r\nBoost\t👩‍💻\x1b\x03".into()),
        Overlay::Preset,
        false,
        AREA
    ));
    assert_eq!(search.query(), "Bass  Boost 👩‍💻");
    assert!(!search.query().chars().any(char::is_control));
    assert!(search.matches("bass boost 👩‍💻"));
    assert!(!search.handle_input(
        &Event::Key(key(KeyCode::Enter)),
        Overlay::Preset,
        false,
        AREA
    ));
}

#[test]
fn paste_requires_an_explicit_visible_current_search() {
    let event = Event::Paste("scene:dialogue".into());
    let mut inactive = Search::default();
    assert!(!inactive.handle_input(&event, Overlay::Preset, false, AREA));
    assert_eq!(inactive.query(), "");
    for overlay in [
        Overlay::None,
        Overlay::Help,
        Overlay::StartSystem,
        Overlay::Proposal,
    ] {
        let mut search = search("Bass");
        assert!(!search.handle_input(&event, overlay, false, AREA));
        assert_eq!(search.query(), "Bass");
    }
    for (invalidated, area) in [
        (true, AREA),
        (false, Rect::new(0, 0, 29, 40)),
        (false, Rect::new(0, 0, 140, 7)),
    ] {
        for overlay in [Overlay::Preset, Overlay::Output] {
            let mut search = search("Bass");
            assert!(!search.handle_input(&event, overlay, invalidated, area));
            assert_eq!(search.query(), "Bass");
        }
    }
}

#[test]
fn oversized_paste_is_rejected_atomically_and_requires_an_edit_to_recover() {
    for text in ["x".repeat(257), "界".repeat(100), "x".repeat(1_048_576)] {
        let mut search = search("Bass");
        assert!(search.handle_input(&Event::Paste(text), Overlay::Output, false, AREA));
        assert_eq!(
            search.query(),
            "Bass",
            "oversized paste partially changed the query"
        );
        assert!(search.warning().is_some());
        assert!(!search.hint(true).contains("Enter"));
        assert!(rendered(&search, 60).contains("too long"));
        search.text_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert!(search.warning().is_none());
        assert!(search.query().is_empty());
    }
}

#[test]
fn empty_or_control_only_paste_does_not_clear_a_rejected_input() {
    for text in ["", "\x1b\x03\x00"] {
        let mut search = search("Bass");
        search.handle_input(&Event::Paste("x".repeat(257)), Overlay::Preset, false, AREA);
        assert!(search.warning().is_some());
        assert!(search.handle_input(&Event::Paste(text.into()), Overlay::Preset, false, AREA));
        assert_eq!(search.query(), "Bass");
        assert!(
            search.warning().is_some(),
            "a paste with no accepted text cleared the confirmation block"
        );
    }
}

#[test]
fn exact_byte_bound_accepts_multibyte_text_and_never_splits_a_pasted_cluster() {
    let mut search = search(&"a".repeat(253));
    search.handle_input(&Event::Paste("界".into()), Overlay::Preset, false, AREA);
    assert_eq!(search.query().len(), 256);
    assert!(search.warning().is_none());
    let before = search.query().to_owned();
    search.handle_input(&Event::Paste("👩‍💻".into()), Overlay::Preset, false, AREA);
    assert_eq!(search.query(), before);
    assert!(search.warning().is_some());
    search.text_key(key(KeyCode::Backspace));
    assert_eq!(search.query().len(), 253);
    assert!(search.warning().is_none());
}

#[test]
fn cached_query_matches_an_independent_reference_across_edit_sequences() {
    let labels = [
        "Bass BOOST",
        "Studio 日本語",
        "automatic",
        "Café 🎧",
        "",
        "\x1bcontrol",
    ];
    let mut search = search("");
    let mut seed = 17u64;
    for step in 0..2000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        match seed % 6 {
            0 => {
                search.text_key(key(KeyCode::Backspace));
            }
            1 => {
                search.text_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            }
            2 => {
                search.handle_input(
                    &Event::Paste("BASS\tBoost".into()),
                    Overlay::Preset,
                    false,
                    AREA,
                );
            }
            3 => {
                search.text_key(key(KeyCode::Esc));
                search.text_key(key(KeyCode::Char('/')));
            }
            _ => {
                search.text_key(key(KeyCode::Char(
                    ['A', 's', ' ', '日', 'é'][(seed >> 8) as usize % 5],
                )));
            }
        }
        for label in labels {
            let clean: String = label.chars().filter(|ch| !ch.is_control()).collect();
            let expected = search
                .query()
                .to_lowercase()
                .split_whitespace()
                .all(|word| clean.to_lowercase().contains(word));
            assert_eq!(
                search.matches(label),
                expected,
                "step={step}, query={:?}",
                search.query()
            );
        }
    }
}

#[test]
fn narrow_search_rendering_stays_inside_the_terminal_cell_budget() {
    for query in [
        "",
        "👩‍💻".repeat(20).as_str(),
        "日本語の長いデバイス名と検索文字列",
    ] {
        let search = search(query);
        for width in 1..80 {
            let text = rendered(&search, width);
            assert!(
                Span::raw(&text).width() <= usize::from(width),
                "width={width}: {text:?}"
            );
            if width >= 30 {
                assert!(text.contains('▏'));
                assert!(text.contains("2/46"));
            }
        }
    }
}

#[test]
fn paste_warning_is_localized_in_every_supported_language() {
    let key = "Search text is too long (256 bytes max)";
    assert!(maris::i18n::has_translation(key));
    for language in maris::i18n::LANGUAGES.iter().skip(1) {
        assert_ne!(maris::i18n::for_language(language, key), key);
    }
}
