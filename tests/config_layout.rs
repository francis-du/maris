use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use maris::{
    configuration::{self, Context, Editor, Outcome},
    control_panel::{Overlay, Workspace},
    device_profile::Capability,
    listening,
    store::Store,
    studio_controls::{self, PointerAction},
    tui_view::{self, Console},
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use serde_json::json;

fn event(rect: Rect, kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}
fn with_view(f: impl FnOnce(Console<'_>)) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let runtime = json!({"active":true,"session_id":"fixture","profile_key":"Fixture Headphones",
        "output":"Fixture Headphones","sample_rate":48000,"updated_at_ms":maris::analysis::now_ms(),
        "device_capability":Capability::default()});
    store.write_json("runtime.json", &runtime).unwrap();
    let snapshot = store.load().unwrap();
    let library = listening::load(&store).unwrap();
    let mut editor = Editor::default();
    let mut row = 16;
    editor
        .handle(
            &store,
            Context {
                runtime: &runtime,
                eq: &snapshot,
                listening: &library,
            },
            &mut row,
            KeyCode::Char('+'),
        )
        .unwrap();
    let applications = json!({});
    let models = maris::neural::models();
    let presets = maris::presets::console_catalog();
    f(Console {
        snapshot: &snapshot,
        configuration: Some(&editor),
        runtime: &runtime,
        workspace: Workspace::Sound,
        sound_row: 16,
        home_row: 0,
        sound_scroll: 0,
        app_row: 0,
        pending_apps: &[],
        devices: &[],
        applications: &applications,
        selected_output: None,
        output_choice: 0,
        presets: &presets,
        preset_choice: 0,
        models: &models,
        music: &library.default,
        palette: maris::theme::VIOLET,
        notice: "OFFLINE FIXTURE",
        goal: "balanced",
        proposal: None,
        overlay: Overlay::None,
    });
    assert_eq!(store.load().unwrap().revision, 0);
}

#[test]
fn browser_has_no_embedded_mutation_targets_even_on_the_selected_row() {
    with_view(|mut view| {
        for (w, h) in [(90, 26), (100, 32), (140, 40), (180, 50)] {
            let area = Rect::new(0, 0, w, h);
            let a = configuration::layout(studio_controls::shell(area).content);
            for selected in configuration::ROW_ORDER {
                view.sound_row = selected;
                for (id, rect) in configuration::browser_rows(a.browser, selected) {
                    for x in rect.x..rect.right() {
                        let click = MouseEvent {
                            column: x,
                            ..event(rect, MouseEventKind::Down(MouseButton::Left))
                        };
                        assert_eq!(
                            configuration::pointer_action(area, &view, click),
                            Some(PointerAction::SelectSound(id))
                        );
                    }
                }
            }
        }
    });
}

#[test]
fn apply_cancel_and_edit_controls_have_separate_nonoverlapping_fixed_regions() {
    for (w, h) in [(90, 26), (100, 32), (140, 40), (180, 50)] {
        let content = studio_controls::shell(Rect::new(0, 0, w, h)).content;
        let a = configuration::layout(content);
        let regions = [
            a.browser, a.target, a.analyzer, a.station, a.review, a.apply, a.cancel,
        ];
        for (index, rect) in regions.iter().enumerate() {
            assert!(
                rect.x >= content.x
                    && rect.y >= content.y
                    && rect.right() <= content.right()
                    && rect.bottom() <= content.bottom(),
                "{w}x{h}: {rect:?}"
            );
            for other in &regions[index + 1..] {
                assert!(!rect.intersects(*other), "{w}x{h}: {rect:?} / {other:?}");
            }
        }
        assert!(a.cancel.x >= a.apply.right() + 3);
        assert!(a.apply.height >= 1 && a.cancel.height >= 1);
    }
}

#[test]
fn separated_plus_stages_and_apply_is_the_only_pointer_that_can_commit() {
    with_view(|view| {
        let area = Rect::new(0, 0, 140, 40);
        let a = configuration::layout(studio_controls::shell(area).content);
        for (rect, key) in [
            (a.plus, KeyCode::Char('+')),
            (a.minus, KeyCode::Char('-')),
            (a.q_plus, KeyCode::Char(']')),
            (a.q_minus, KeyCode::Char('[')),
            (a.cancel, KeyCode::Esc),
        ] {
            assert_eq!(
                configuration::pointer_action(
                    area,
                    &view,
                    event(rect, MouseEventKind::Down(MouseButton::Left))
                ),
                Some(PointerAction::Key(key))
            );
        }
        let mut pointer = configuration::PointerLatch::default();
        let down = event(a.apply, MouseEventKind::Down(MouseButton::Left));
        let up = event(a.apply, MouseEventKind::Up(MouseButton::Left));
        assert_eq!(configuration::pointer_action(area, &view, down), None);
        assert_eq!(pointer.handle(area, &view, down), None);
        assert_eq!(
            pointer.handle(area, &view, up),
            Some(PointerAction::Key(KeyCode::Enter))
        );
        assert_eq!(pointer.handle(area, &view, up), None);
        for kind in [
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
        ] {
            assert_eq!(
                configuration::pointer_action(area, &view, event(a.apply, kind)),
                None
            );
        }
        assert_eq!(
            configuration::pointer_action(
                area,
                &Console {
                    overlay: Overlay::Help,
                    ..view
                },
                event(a.apply, MouseEventKind::Down(MouseButton::Left))
            ),
            None
        );
        assert_eq!(
            configuration::pointer_action(
                Rect::new(0, 0, 80, 24),
                &view,
                event(a.apply, MouseEventKind::Down(MouseButton::Left))
            ),
            None
        );
    });
}

#[test]
fn actual_render_shows_current_draft_scope_and_cancel_without_claiming_audio_application() {
    with_view(|view| {
        for (w, h) in [(100, 32), (140, 40), (180, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            for label in [
                "Sound settings",
                "Current",
                "Draft",
                "Pending changes",
                "Enter Apply changes",
                "Esc Cancel draft",
                "sound unchanged",
                "Global EQ",
                "OFFLINE FIXTURE",
            ] {
                assert!(text.contains(label), "{w}x{h}: missing {label}");
            }
            assert!(!text.contains("Space A/B"));
        }
    });
}

#[test]
fn pending_draft_is_visible_but_cannot_be_clicked_through_compact_or_stale_state() {
    with_view(|view| {
        let area = Rect::new(0, 0, 80, 24);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Draft pending"));
        assert!(text.contains("Esc cancels"));
        assert!(!text.contains("Enter Apply changes"));
        assert!(configuration::pointer_action(
            area,
            &view,
            event(
                Rect::new(5, 20, 1, 1),
                MouseEventKind::Down(MouseButton::Left)
            )
        )
        .is_none());
        let stale = json!({"active":false});
        let stale_view = Console {
            runtime: &stale,
            ..view
        };
        let large = Rect::new(0, 0, 140, 40);
        let controls = configuration::layout(studio_controls::shell(large).content);
        for target in [controls.plus, controls.apply] {
            assert!(configuration::pointer_action(
                large,
                &stale_view,
                event(target, MouseEventKind::Down(MouseButton::Left))
            )
            .is_none());
        }
    });
}

#[test]
fn apply_pointer_requires_a_complete_click_on_the_same_visible_draft() {
    with_view(|view| {
        let area = Rect::new(0, 0, 140, 40);
        let a = configuration::layout(studio_controls::shell(area).content);
        let down = event(a.apply, MouseEventKind::Down(MouseButton::Left));
        let up = event(a.apply, MouseEventKind::Up(MouseButton::Left));
        let mut pointer = configuration::PointerLatch::default();
        assert!(pointer.handle(area, &view, up).is_none());
        for kind in [
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::ScrollDown,
        ] {
            assert!(pointer.handle(area, &view, down).is_none());
            pointer.handle(area, &view, event(a.apply, kind));
            assert!(pointer.handle(area, &view, up).is_none());
        }
        pointer.handle(area, &view, down);
        pointer.reset(); // Keyboard or resize interrupts an incomplete click.
        assert!(pointer.handle(area, &view, up).is_none());
        pointer.handle(area, &view, down);
        assert!(pointer
            .handle(
                area,
                &Console {
                    sound_row: 17,
                    ..view
                },
                up
            )
            .is_none());
        pointer.handle(area, &view, down);
        assert!(pointer
            .handle(
                area,
                &Console {
                    overlay: Overlay::Help,
                    ..view
                },
                up
            )
            .is_none());
        pointer.handle(area, &view, down);
        let small = Rect::new(0, 0, 80, 24);
        assert!(pointer.handle(small, &view, up).is_none());
        pointer.handle(area, &view, down);
        assert!(pointer
            .handle(
                area,
                &view,
                event(a.cancel, MouseEventKind::Up(MouseButton::Left))
            )
            .is_none());
        pointer.handle(area, &view, down);
        assert_eq!(
            pointer.handle(area, &view, up),
            Some(PointerAction::Key(KeyCode::Enter))
        );
        assert!(pointer.handle(area, &view, up).is_none());
    });
}

#[test]
fn playback_switches_are_visibly_separate_and_header_comparison_cannot_change_audio() {
    with_view(|view| {
        let area = Rect::new(0, 0, 140, 40);
        let shell = studio_controls::shell(area);
        let comparison = studio_controls::header(shell.header).compare;
        assert!(studio_controls::pointer_action(
            area,
            Workspace::Sound,
            Overlay::None,
            16,
            event(comparison, MouseEventKind::Down(MouseButton::Left))
        )
        .is_none());
        let empty_editor = Editor::default();
        let settings = Console {
            configuration: Some(&empty_editor),
            sound_row: 11,
            ..view
        };
        let a = configuration::layout(shell.content);
        let ids: Vec<_> = configuration::browser_rows(a.browser, 11)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec![14, 15, 11]);
        assert!(!ids.iter().any(|id| *id >= 16));
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal
            .draw(|frame| tui_view::draw(frame, &settings))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        for label in [
            "Device sound",
            "Global EQ",
            "Playback switches",
            "Skip audio effects",
            "OFF",
            "ON",
            "clipping protection stays on",
        ] {
            assert!(text.contains(label), "missing {label}");
        }
        for (rect, group) in configuration::group_rects(a.browser)
            .into_iter()
            .zip(configuration::Group::ALL)
        {
            assert_eq!(
                configuration::pointer_action(
                    area,
                    &settings,
                    event(rect, MouseEventKind::Down(MouseButton::Left))
                ),
                Some(PointerAction::Key(group.key()))
            );
        }
    });
}

#[test]
fn source_event_loop_routes_settings_to_draft_reducer_before_live_key_dispatch() {
    let source = include_str!("../src/ui/tui/mod.rs");
    assert!(
        source.find("configuration.handle(").unwrap()
            < source.find("if control_panel::changes_sound(").unwrap()
    );
    let compact: String = source.split_whitespace().collect();
    assert!(compact.contains("configuration.pending()&&control_panel::compact_layout"));
    let _ = Outcome::Browse;
}
