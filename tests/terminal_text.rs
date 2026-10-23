//! Terminal-cell truncation must preserve complete displayed graphemes.

#[test]
fn terminal_test_parser_survives_resize_through_a_wide_glyph_then_erase() {
    // Exercise the same resize/erase sequence used when a small TUI is redrawn.
    // Upstream reference: https://github.com/doy/vt100-rust/issues/28
    let rows = std::num::NonZeroU16::new(3).unwrap();
    let wide = std::num::NonZeroU16::new(8).unwrap();
    let narrow = std::num::NonZeroU16::new(1).unwrap();
    for glyph in ["界", "音", "🎧"] {
        let mut parser = vt100::Parser::new(rows, wide, 0);
        parser.process(glyph.as_bytes());
        parser.screen_mut().set_size(rows, narrow);
        parser.process(b"\x1b[K\rX");
        assert!(parser.screen().contents().contains('X'));
        parser.screen_mut().set_size(rows, wide);
        parser.process(b"\r\x1b[2Kready");
        assert!(parser.screen().contents().contains("ready"));
    }
}

use maris::studio_view::fit;
use ratatui::{style::Style, text::Span};

#[test]
fn truncated_labels_preserve_emoji_and_combining_clusters() {
    for (text, width, expected) in [
        ("👩‍💻 output", 3, "👩‍💻…"),
        ("🇨🇳 speakers", 3, "🇨🇳…"),
        ("e\u{301}q headphones", 2, "e\u{301}…"),
        ("界面音频", 3, "界…"),
    ] {
        assert_eq!(fit(text, width), expected, "{text:?} at {width} cells");
    }
}

#[test]
fn every_truncated_label_is_a_grapheme_prefix_within_its_cell_budget() {
    for text in [
        "👨‍👩‍👧‍👦 Family output",
        "🇯🇵 日本語",
        "a\u{301}\u{308}b label",
        "🎧 Studio",
        "plain ASCII",
    ] {
        let span = Span::raw(text);
        let mut prefixes = vec![String::new()];
        for grapheme in span.styled_graphemes(Style::default()) {
            prefixes.push(format!("{}{}", prefixes.last().unwrap(), grapheme.symbol));
        }
        for width in 0..24 {
            let result = fit(text, width);
            assert!(Span::raw(&result).width() <= usize::from(width));
            if result != text && !result.is_empty() {
                let prefix = result
                    .strip_suffix('…')
                    .expect("truncated label has ellipsis");
                assert!(
                    prefixes.iter().any(|candidate| candidate == prefix),
                    "split grapheme: {result:?}"
                );
            }
        }
    }
}
