//! Confirmation dispatch with isolated state; never creates an audio session.
use super::*;
use crossterm::event::{KeyEvent, KeyModifiers};

fn choose_preset(no_matches: bool, visible_rows: usize) -> (u64, Overlay) {
    choose_preset_after_input(no_matches, visible_rows, false)
}

fn choose_preset_after_input(
    no_matches: bool,
    visible_rows: usize,
    rejected_paste: bool,
) -> (u64, Overlay) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let runtime = serde_json::json!({"active":false});
    let snapshot = Snapshot::default();
    let presets = crate::presets::console_catalog();
    let chosen = presets
        .iter()
        .position(|preset| {
            !matches!(preset.category, "scene" | "listening")
                && !crate::presets::tone_curve_matches(&snapshot.profile, &preset.id, 48_000)
        })
        .expect("fixture must select a changed tone curve");
    let mut search = Search::default();
    let query = if no_matches {
        "zzzz-no-matching-preset"
    } else {
        &presets[chosen].id
    };
    search.text_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for ch in query.chars() {
        search.text_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    if rejected_paste {
        assert!(search.handle_input(
            &crossterm::event::Event::Paste("x".repeat(257)),
            Overlay::Preset,
            false,
            ratatui::layout::Rect::new(0, 0, 140, 40),
        ));
        assert!(search.warning().is_some());
    }
    let mut overlay = Overlay::Preset;
    let mut pin = false;
    let mut index = None;
    let mut output_choice = 2;
    let mut preset_choice = chosen;
    let mut undo = UndoTarget::Profile;
    let mut proposal = None;
    let mut notice = Notice::new("Ready");
    handle(
        KeyCode::Enter,
        Context {
            store: &store,
            runtime: &runtime,
            snapshot: &snapshot,
            listening_revision: 0,
            profile_key: None,
            outputs: &[],
            picker_outputs: &[],
            presets: &presets,
        },
        State {
            overlay: &mut overlay,
            pin_output: &mut pin,
            output_index: &mut index,
            output_choice: &mut output_choice,
            preset_choice: &mut preset_choice,
            undo_target: &mut undo,
            proposal: &mut proposal,
            notice: &mut notice,
        },
        &search,
        visible_rows,
    )
    .unwrap();
    assert!(!directory.path().join("control.json").exists());
    assert!(!pin && index.is_none());
    let saved = store.load().unwrap();
    if saved.revision > 0 {
        let expected =
            crate::presets::apply_tone_curve(&snapshot.profile, &presets[chosen].id, 48_000)
                .unwrap();
        assert_eq!(
            serde_json::to_value(saved.profile).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    (saved.revision, overlay)
}

#[test]
fn rejected_paste_cannot_confirm_the_previous_query_or_consume_undo() {
    assert_eq!(
        choose_preset_after_input(false, 4, true),
        (0, Overlay::Preset)
    );
}

#[test]
fn empty_results_never_confirm_the_stale_canonical_choice() {
    assert_eq!(choose_preset(true, 4), (0, Overlay::Preset));
}

#[test]
fn invisible_rows_cannot_be_confirmed_by_keyboard() {
    assert_eq!(choose_preset(false, 0), (0, Overlay::Preset));
}

#[test]
fn visible_filtered_choice_preserves_original_preset_identity() {
    assert_eq!(choose_preset(false, 1), (1, Overlay::None));
}
