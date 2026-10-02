use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use maris::{
    control_panel::{CursorMemory, Overlay, Workspace, EQ_ROW_START},
    inspector,
    store::Snapshot,
    studio_controls::{self as controls, PointerAction},
    tui_view::{self, Console},
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use serde_json::json;

fn click(rect: Rect) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}
fn with_view(test: impl FnOnce(Console<'_>)) {
    let snapshot = Snapshot::default();
    let runtime = json!({"active":false});
    let applications = json!({"available":true,"applications":[
        {"pid":0,"bundle_id":"invalid"},
        {"pid":10,"bundle_id":"first","is_maris":false},
        {"pid":20,"bundle_id":"second","is_maris":false}
    ]});
    let models = maris::neural::models();
    let presets = maris::presets::list();
    let music = maris::music::MusicProfile::default();
    test(Console {
        snapshot: &snapshot,
        configuration: None,
        runtime: &runtime,
        workspace: Workspace::Sound,
        sound_row: EQ_ROW_START,
        home_row: 2,
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
        music: &music,
        palette: maris::theme::VIOLET,
        notice: "FIXTURE",
        goal: "balanced",
        proposal: None,
        overlay: Overlay::None,
    });
}

#[test]
fn app_list_shows_linux_and_windows_names_and_does_not_deny_working_mixer_controls() {
    with_view(|view| {
        let apps = json!({"available":true,"applications":[
            {"pid":10,"bundle_id":null,"name":"Windows Player"},
            {"pid":20,"bundle_id":"","display_name":"Linux Player"},
            {"pid":30,"bundle_id":"com.example.fallback"}
        ]});
        let view = Console {
            applications: &apps,
            workspace: Workspace::Apps,
            ..view
        };
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        for name in ["Windows Player", "Linux Player", "com.example.fallback"] {
            assert!(text.contains(name), "app name disappeared: {name}");
        }
        assert!(!text.contains("independent app gain unavailable"));
    });
}

#[test]
fn active_mixer_uses_its_own_channels_meters_and_pointer_actions() {
    with_view(|view| {
        let mut runtime = json!({
            "active":true, "system_backend":"multi_device_mixer", "output":"Fixture output",
            "updated_at_ms":maris::analysis::now_ms(), "sample_rate":48000,
            "mixer_control":{"revision":4,"config":{"strips":[
                {"id":"first", "name":"Music channel", "source_device":"pid:77", "gain_db":-3.0,"pan":0.0,"mute":false,"solo":true},
                {"id":"second", "name":"Voice channel", "source_device":"pid:88", "gain_db":-6.0,"pan":0.1,"mute":true,"solo":false}
            ]}},
            "mixer":{"revision":4,"processing_revision":3,"restart_required":false,
                "strips":[{"id":"second","peak_dbfs":-24.25},{"id":"first","peak_dbfs":-13.75}]}
        });
        let render = |runtime: &serde_json::Value| {
            let view = Console {
                runtime,
                workspace: Workspace::Apps,
                ..view
            };
            let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
            terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        let text = render(&runtime);
        for label in [
            "Music channel",
            "Voice channel",
            "-13.75",
            "-24.25",
            "Space Mute",
            "Not applied yet",
        ] {
            assert!(text.contains(label), "missing {label}");
        }
        assert!(!text.contains("Enter Apply selection"));
        let area = Rect::new(0, 0, 140, 40);
        let work = inspector::inset(inspector::layout(controls::shell(area).content).work);
        let current = Console {
            runtime: &runtime,
            workspace: Workspace::Apps,
            ..view
        };
        assert_eq!(
            inspector::pointer_action(area, &current, click(inspector::command_rects(work)[1])),
            Some(PointerAction::Key(KeyCode::Char(' ')))
        );
        assert_eq!(
            inspector::pointer_action(area, &current, click(inspector::command_rects(work)[2])),
            Some(PointerAction::Key(KeyCode::Char('x')))
        );
        runtime["updated_at_ms"] = json!(0);
        let stale = render(&runtime);
        assert!(!stale.contains("-13.75"));
        assert!(!stale.contains("-24.25"));
        let current = Console {
            runtime: &runtime,
            workspace: Workspace::Apps,
            ..view
        };
        assert_eq!(
            inspector::pointer_action(area, &current, click(inspector::command_rects(work)[1])),
            None
        );
    });
}

#[test]
fn mixer_control_labels_are_present_in_all_six_languages() {
    for key in [
        "Mixer controls",
        "No mixer channels",
        "Space Mute",
        "X Solo",
        "Previous mixer change undone",
        "←/→ volume · Space mute · X solo · [/] balance · U undo",
    ] {
        assert!(maris::i18n::has_translation(key), "{key}");
        for language in maris::i18n::LANGUAGES {
            assert!(!maris::i18n::for_language(language, key).is_empty());
        }
    }
}

#[test]
fn every_inspector_is_one_visible_action_from_every_other_view() {
    let area = Rect::new(0, 0, 140, 40);
    for from in Workspace::ALL {
        for (rect, to) in controls::actions(controls::shell(area).actions)
            .into_iter()
            .zip(Workspace::ALL)
        {
            assert_eq!(Workspace::direct(to.shortcut()), Some(to));
            assert_eq!(
                controls::pointer_action(area, from, Overlay::None, 0, click(rect)),
                Some(PointerAction::Navigate(to))
            );
        }
    }
}

#[test]
fn cursor_memory_does_not_require_walking_past_sixteen_rows_to_reach_eq() {
    let mut memory = CursorMemory::default();
    let mut row = memory.visit(Workspace::Now, Workspace::Sound, 9);
    assert_eq!(row, EQ_ROW_START);
    row = EQ_ROW_START + 8;
    memory.visit(Workspace::Sound, Workspace::Intelligence, row);
    assert_eq!(
        memory.visit(Workspace::Intelligence, Workspace::Sound, row),
        row
    );
    assert_eq!(memory.visit(Workspace::Sound, Workspace::Now, row), 9);
    memory.visit(Workspace::Sound, Workspace::Now, 3);
    assert_eq!(memory.home, 3);
    assert_eq!(memory.eq, EQ_ROW_START + 8);
}

#[test]
fn inspector_and_home_share_the_fixed_rail_without_overlapping_action_regions() {
    for (width, height) in [(90, 26), (100, 32), (140, 40), (180, 50)] {
        let area = Rect::new(0, 0, width, height);
        let shell = controls::shell(area);
        let home = maris::studio_view::layout(shell.content);
        let advanced = inspector::layout(shell.content);
        assert_eq!(home.rail, advanced.rail);
        assert!(advanced.work.x >= advanced.rail.right());
        // Sound settings now has its own read-only browser and draft station.
        // config_layout.rs checks its actual geometry and commit targets.
        assert!(advanced.work.right() <= shell.content.right());
        assert!(advanced.work.bottom() <= shell.content.bottom());
    }
}

#[test]
fn visible_listening_rail_plus_remains_usable_inside_inspectors() {
    with_view(|mut view| {
        let area = Rect::new(0, 0, 140, 40);
        let rail_area = inspector::layout(controls::shell(area).content).rail;
        for workspace in Workspace::DETAILS {
            if workspace == Workspace::Sound {
                continue;
            }
            view.workspace = workspace;
            let rail = controls::rail(rail_area, view.home_row);
            let parts = controls::parameter(rail.parameter(view.home_row).unwrap());
            assert_eq!(
                inspector::pointer_action(area, &view, click(parts.increment)),
                Some(PointerAction::AdjustListening {
                    row: view.home_row,
                    direction: 1
                })
            );
            assert_eq!(
                inspector::pointer_action(area, &view, click(parts.label)),
                Some(PointerAction::SelectHomeSound(view.home_row))
            );
        }
    });
}

#[test]
fn application_checkboxes_stage_only_and_apply_is_a_distinct_visible_target() {
    with_view(|mut view| {
        view.workspace = Workspace::Apps;
        let area = Rect::new(0, 0, 100, 32);
        let work = inspector::inset(inspector::layout(controls::shell(area).content).work);
        let list = inspector::app_rows(work);
        assert_eq!(
            inspector::pointer_action(area, &view, click(list)),
            Some(PointerAction::ToggleApp(0))
        );
        assert_eq!(
            inspector::pointer_action(area, &view, click(Rect::new(list.x + 6, list.y, 1, 1))),
            Some(PointerAction::SelectApp(0))
        );
        assert_eq!(
            inspector::pointer_action(area, &view, click(inspector::command_rects(work)[1])),
            Some(PointerAction::Key(KeyCode::Enter))
        );
        assert_eq!(
            inspector::pointer_action(area, &view, click(inspector::command_rects(work)[0])),
            Some(PointerAction::Key(KeyCode::Char('a')))
        );
        assert!(view.pending_apps.is_empty());
        assert!(inspector::app_offset(500, 501, 8) <= 500);
    });
}

#[test]
fn assist_goal_tiles_request_preview_and_never_apply_or_download() {
    with_view(|mut view| {
        view.workspace = Workspace::Intelligence;
        let area = Rect::new(0, 0, 100, 32);
        let work = inspector::inset(inspector::layout(controls::shell(area).content).work);
        for (index, rect) in inspector::goal_rects(work).into_iter().enumerate() {
            assert_eq!(
                inspector::pointer_action(area, &view, click(rect)),
                Some(PointerAction::PreviewGoal(index))
            );
            let blocked = Console {
                overlay: Overlay::Proposal,
                ..view
            };
            assert_eq!(inspector::pointer_action(area, &blocked, click(rect)), None);
        }
        assert_eq!(
            inspector::pointer_action(area, &view, click(inspector::command_rects(work)[2])),
            Some(PointerAction::Key(KeyCode::Char('u')))
        );
    });
}

#[test]
fn dialogs_compact_mode_and_drags_cannot_reach_inspector_controls() {
    with_view(|mut view| {
        let area = Rect::new(0, 0, 140, 40);
        let settings = maris::configuration::layout(controls::shell(area).content);
        let event = click(settings.plus);
        for overlay in [
            Overlay::Output,
            Overlay::Preset,
            Overlay::Proposal,
            Overlay::Help,
            Overlay::StartSystem,
        ] {
            view.overlay = overlay;
            assert_eq!(inspector::pointer_action(area, &view, event), None);
            assert_eq!(
                controls::pointer_action(area, view.workspace, overlay, view.sound_row, event),
                None
            );
        }
        view.overlay = Overlay::None;
        assert_eq!(
            inspector::pointer_action(Rect::new(0, 0, 80, 24), &view, event),
            None
        );
        for kind in [
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::Up(MouseButton::Left),
        ] {
            assert_eq!(
                inspector::pointer_action(area, &view, MouseEvent { kind, ..event }),
                None
            );
        }
    });
}

#[test]
fn all_inspectors_render_without_reintroducing_the_old_nested_card_ui() {
    with_view(|mut view| {
        for workspace in Workspace::DETAILS {
            view.workspace = workspace;
            for (width, height) in [(100, 32), (140, 40)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                for label in [
                    "Esc Studio",
                    "E Settings",
                    "M Apps",
                    "V Device",
                    "I Assist",
                    "H Health",
                    "FIXTURE",
                ] {
                    assert!(
                        text.contains(label),
                        "{workspace:?} {width}x{height} missing {label}"
                    );
                }
                assert!(text.contains("Sound"));
                assert!(!text.contains('╭'));
            }
        }
    });
}
