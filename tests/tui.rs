use maris::{
    audio::DeviceInfo,
    control_panel::{Overlay, Workspace},
    profile::Profile,
    store::Snapshot,
    tui_view::{self, Console},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};

fn text_of(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn devices() -> Vec<DeviceInfo> {
    vec![
        DeviceInfo {
            id: "uid:BuiltInSpeakerDevice".into(),
            name: "MacBook Air Speakers".into(),
            direction: "output".into(),
            is_default: true,
        },
        DeviceInfo {
            id: "uid:FixtureHeadphones".into(),
            name: "Studio Headphones".into(),
            direction: "output".into(),
            is_default: false,
        },
    ]
}

fn runtime() -> Value {
    json!({
        "active":true,
        "updated_at_ms":maris::analysis::now_ms(),
        "visualization":{"updated_at_ms":maris::analysis::now_ms()},
        "output":"MacBook Air Speakers",
        "output_mode":"follow_system_default",
        "sample_rate":48000,
        "peak_left_dbfs":-9.0,
        "peak_right_dbfs":-10.0,
        "effective_preamp_db":-2.0,
        "engine_revision":"test",
        "rebind_count":1,
        "reported_output_path_latency_ms":4.25,
        "system_backend":"coreaudio_process_tap",
        "capture_scope":"system_playback_excluding_maris",
        "device_identity":{"stable_id":"BuiltInSpeakerDevice","model_id":"AppleUSBAudioEngineModel"},
        "profile_binding_source":"stable_id",
        "profile_key":"MacBook Air Speakers",
        "device_binding_revision":2,
        "underruns":0,
        "overruns":0,
        "performance":{
            "callback_processing_average_percent":1.75,
            "callback_processing_max_us":420.0
        },
        "display_bands":vec![0.4;24],
        "captured_application_pids":[222],
        "requested_music_revision":0,"applied_music_revision":0,"applied_revision":0,
        "analysis":{"momentary_lufs":-18.0,"true_peak_dbtp":-3.0,"sample_rate":48000,"updated_at_ms":maris::analysis::now_ms()}
    })
}

fn applications() -> Value {
    json!({
        "available":true,
        "applications":[
            {"pid":111,"bundle_id":"com.example.alpha","running_output":true,"devices":["Studio Headphones"],"is_maris":false},
            {"pid":222,"bundle_id":"com.example.beta","running_output":false,"devices":[],"is_maris":false}
        ]
    })
}

fn render(workspace: Workspace, overlay: Overlay, width: u16, height: u16) -> String {
    render_with(workspace, overlay, width, height, |_| {})
}

fn render_with(
    workspace: Workspace,
    overlay: Overlay,
    width: u16,
    height: u16,
    customize: impl FnOnce(&mut Console<'_>),
) -> String {
    let runtime = runtime();
    let now = runtime["updated_at_ms"].as_u64().unwrap();
    let _clock = maris::analysis::DebugClock::freeze_at(now);
    render_with_runtime(workspace, overlay, width, height, runtime, customize)
}

fn render_with_runtime(
    workspace: Workspace,
    overlay: Overlay,
    width: u16,
    height: u16,
    runtime: Value,
    customize: impl FnOnce(&mut Console<'_>),
) -> String {
    let snapshot = Snapshot::default();
    let devices = devices();
    let applications = applications();
    let presets = maris::presets::list();
    let models = maris::neural::models();
    let music = maris::music::MusicProfile::default();
    let pending = vec![111_i32];
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut view = Console {
        snapshot: &snapshot,
        configuration: None,
        runtime: &runtime,
        workspace,
        sound_row: if workspace == Workspace::Sound { 16 } else { 1 },
        home_row: 1,
        sound_scroll: if workspace == Workspace::Sound { 2 } else { 0 },
        app_row: 0,
        pending_apps: &pending,
        devices: &devices,
        applications: &applications,
        selected_output: devices.first(),
        output_choice: 2,
        presets: &presets,
        preset_choice: 0,
        models: &models,
        music: &music,
        palette: maris::theme::VIOLET,
        notice: "Ready",
        goal: "balanced",
        proposal: None,
        overlay,
    };
    customize(&mut view);
    terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
    text_of(&terminal)
}

#[test]
fn default_dashboard_has_no_required_workspace_tabs() {
    let dashboard = render(Workspace::Now, Overlay::None, 180, 50);
    assert!(dashboard.contains("Dashboard"));
    for removed in ["1 NOW", "2 SOUND", "3 APPS", "4 SYSTEM"] {
        assert!(!dashboard.contains(removed));
    }
    for detail in Workspace::DETAILS {
        let text = render(detail, Overlay::None, 130, 36);
        assert!(text.contains(maris::i18n::text(detail.title())));
        assert!(text.contains("Esc Studio"));
    }
}

#[test]
fn now_is_a_single_glanceable_control_surface() {
    let text = render(Workspace::Now, Overlay::None, 180, 50);
    for section in [
        "Device",
        "Real-time spectrum",
        "Profile EQ",
        "Output",
        "dBFS",
        "Sound",
        "Bass Assist",
        "Dynamic EQ",
        "Music context",
        "Listening Assist",
        "Mixer summary",
        "System health",
        "LUFS",
        "TP",
        "Theme source",
    ] {
        assert!(
            text.contains(maris::i18n::text(section)),
            "missing {section}"
        );
    }
    assert!(text.contains("O output"));
    assert!(
        !text.contains('╭'),
        "default screen must not return to equal rounded cards"
    );
    assert!(text.contains("Bass"));
    assert!(text.contains("Presence"));
}

#[test]
fn sound_settings_separates_browsing_from_draft_and_explicit_apply() {
    let text = render(Workspace::Sound, Overlay::None, 130, 36);
    assert!(text.contains("Profile EQ"));
    assert!(text.contains("Preamp"));
    assert!(text.contains("Crossfeed"));
    assert!(!text.contains("SOUND · one cursor"));
    assert!(text.contains("EQ"));
    assert!(text.contains("Current"));
    assert!(text.contains("Draft"));
    assert!(text.contains("Enter Apply changes"));
    assert!(!text.contains("Space A/B"));
    assert!(
        text.contains("Esc Studio"),
        "optional detail must have a visible path home"
    );
}

#[test]
fn apps_make_pending_and_active_scope_visible() {
    let text = render(Workspace::Apps, Overlay::None, 130, 36);
    assert!(text.contains("com.example.alpha"));
    assert!(text.contains("com.example.beta"));
    assert!(text.contains("111"));
    assert!(text.contains("222"));
    assert!(text.contains('◆'));
    assert!(text.contains('●'));
    assert!(text.contains("Enter applies once"));
}

#[test]
fn system_shows_measured_runtime_facts_and_model_truthfulness() {
    let text = render(Workspace::System, Overlay::None, 130, 36);
    assert!(text.contains("Engine health"));
    assert!(text.contains("Audio processing time"));
    assert!(text.contains("1.75%"));
    assert!(text.contains("4.25 ms"));
    let assist = render(Workspace::Intelligence, Overlay::None, 130, 36);
    for required in [
        "Listening Assist",
        "Balanced",
        "Warm",
        "Clear",
        "Soft",
        "Built-in processing",
        "B Compare",
        "U Undo",
    ] {
        assert!(
            assist.contains(required),
            "Missing assist action {required}"
        );
    }
    for removed in [
        "musicnn",
        "candidate",
        "clap-music",
        "D Download",
        "AI / Models",
    ] {
        assert!(
            !assist.contains(removed),
            "Research catalog leaked into product UI: {removed}"
        );
    }
}

#[test]
fn output_picker_is_explicit_and_does_not_present_o_as_cycle_and_switch() {
    let text = render(Workspace::Now, Overlay::Output, 130, 36);
    assert!(text.contains("Follow system default"));
    assert!(text.contains("MacBook Air Speakers"));
    assert!(text.contains("Studio Headphones"));
    assert!(text.contains("Enter switches"));
    assert!(!text.contains("Shift+O"));
}

#[test]
fn preset_picker_is_flat_without_category_navigation_modes() {
    let text = render(Workspace::Sound, Overlay::Preset, 130, 36);
    assert!(text.contains("PRESETS · Enter applies"));
    assert!(text.contains("PRESET"));
    assert!(text.contains("GROUP"));
    assert!(!text.contains("Tab Next"));
}

#[test]
fn compact_monitor_still_shows_routing_confirmation_overlay() {
    let text = render(Workspace::Now, Overlay::Output, 50, 14);
    assert!(text.contains("Follow system default"));
    assert!(text.contains("Studio Headphones"));
    assert!(text.contains("Enter switches"));
    assert!(text.contains("Esc cancels"));
}

#[test]
fn preset_selection_remains_visible_after_scrolling_beyond_initial_viewport() {
    let last = maris::presets::list().last().unwrap().name.clone();
    let text = render_with(Workspace::Now, Overlay::Preset, 70, 18, |view| {
        view.preset_choice = view.presets.len() - 1;
    });
    assert!(text.contains(&last), "last preset was selected off-screen");
}

#[test]
fn full_sound_control_rows_match_the_event_dispatch_indices() {
    render_with(Workspace::Sound, Overlay::None, 130, 36, |view| {
        let rows = maris::dashboard::sound_rows(view);
        assert_eq!(rows.len(), maris::control_panel::SOUND_ROWS);
        for (row, expected) in [
            (0, "Bass"),
            (1, "Bass Assist"),
            (5, "Intensity"),
            (6, "Dynamic EQ"),
            (8, "Balance"),
            (9, "Compressor"),
            (10, "Preamp"),
            (14, "Music processing"),
            (15, "Level match"),
            (26, "Virtual 360"),
            (27, "Stereo Focus"),
        ] {
            assert_eq!(rows[row].0, maris::i18n::text(expected), "row {row}");
        }
        assert!(rows[16].0.contains("31"));
        assert!(rows[25].0.contains("16000"));
    });
}

#[test]
fn lowest_sound_rows_scroll_into_view_even_on_short_terminals() {
    let text = render_with(Workspace::Sound, Overlay::None, 96, 30, |view| {
        view.sound_row = 27;
    });
    assert!(text.contains(maris::i18n::text("Stereo Focus")));
    assert!(text.contains('▶'));
}

#[test]
fn tiny_terminals_fall_back_to_compact_monitor() {
    let text = render(Workspace::Now, Overlay::None, 50, 14);
    assert!(text.contains("MARIS"));
    assert!(!text.contains("Mixer summary"));
}

#[test]
fn medium_dashboard_keeps_all_essential_modules_without_navigation() {
    for (width, height) in [(100, 32), (120, 36), (140, 40)] {
        let text = render(Workspace::Now, Overlay::None, width, height);
        for module in [
            "Device",
            "Sound",
            "Real-time spectrum",
            "Profile EQ",
            "Music context",
            "Listening Assist",
            "Mixer summary",
            "System health",
        ] {
            assert!(
                text.contains(maris::i18n::text(module)),
                "{width}x{height} missing {module}"
            );
        }
    }
}

#[test]
fn dashboard_cursor_can_reach_all_ten_listening_controls() {
    for row in 0..maris::control_panel::DASHBOARD_SOUND_ROWS {
        let mut label = String::new();
        let text = render_with(Workspace::Now, Overlay::None, 100, 32, |view| {
            view.sound_row = row;
            label = maris::dashboard::sound_rows(view)[row].0.clone();
        });
        assert!(
            text.contains(&label),
            "dashboard selection {row} hidden: {label}"
        );
        assert!(text.contains('▶'));
    }
}

#[test]
fn stale_semantic_tags_and_inactive_levels_never_appear_as_live_evidence() {
    let runtime = json!({"active":false, "peak_left_dbfs":-1.0, "peak_right_dbfs":-2.0,
        "display_bands":vec![1.0;24], "music_context":{"genre":[{"label":"forged-style","confidence":0.99}],
        "instruments":[{"label":"forged-instrument","confidence":0.99}], "mode":"semantic", "confidence":0.99}});
    let text = render_with_runtime(Workspace::Now, Overlay::None, 180, 50, runtime, |_| {});
    assert!(!text.contains("forged-style"));
    assert!(!text.contains("forged-instrument"));
    assert!(!text.contains("-1.0 dBFS"));
    assert!(text.contains("Waiting for signal"));
    assert!(text.contains("Unknown"));
}

#[test]
fn studio_layout_keeps_the_analyzer_dominant_and_regions_nonoverlapping() {
    use ratatui::layout::Rect;
    for (width, height) in [(90, 26), (100, 32), (120, 36), (140, 40), (180, 50)] {
        let area = Rect::new(1, 4, width - 2, height - 8);
        let layout = maris::studio_view::layout(area);
        assert!(
            layout.analyzer.width * 5 >= area.width * 3,
            "analyzer too small at {width}x{height}"
        );
        let regions = [
            layout.rail,
            layout.analyzer,
            layout.context,
            layout.intelligence,
            layout.mixer,
            layout.health,
        ];
        for (i, a) in regions.iter().enumerate() {
            assert!(
                a.x >= area.x
                    && a.y >= area.y
                    && a.right() <= area.right()
                    && a.bottom() <= area.bottom()
            );
            for b in &regions[i + 1..] {
                assert!(
                    a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y,
                    "regions overlap at {width}x{height}: {a:?} / {b:?}"
                );
            }
        }
    }
}

#[test]
fn studio_unicode_ellipsis_respects_display_cells() {
    use ratatui::text::Span;
    for width in 0..20 {
        let fitted = maris::studio_view::fit("工作室耳机 — a very long device name", width);
        assert!(Span::raw(fitted).width() <= usize::from(width));
    }
}

#[test]
fn studio_rejects_stale_or_future_live_telemetry() {
    assert!(maris::studio_view::live(
        &json!({"active":true,"updated_at_ms":1000}),
        1200
    ));
    assert!(!maris::studio_view::live(
        &json!({"active":true,"updated_at_ms":1000}),
        2500
    ));
    assert!(!maris::studio_view::live(
        &json!({"active":true,"updated_at_ms":3000}),
        2500
    ));
    assert!(!maris::studio_view::live(&json!({"active":true}), 2500));
}

#[test]
fn footer_never_wraps_shortcuts_over_error_or_offline_fixture_notice() {
    for (width, height) in [(90, 26), (100, 32), (140, 40)] {
        let text = render_with(Workspace::Now, Overlay::None, width, height, |view| {
            view.notice = "OFFLINE UI FIXTURE — not a hardware session";
        });
        assert!(
            text.contains("OFFLINE UI FIXTURE"),
            "notice hidden at {width}x{height}"
        );
    }
}

#[test]
fn fresh_runtime_does_not_resurrect_stale_loudness_analysis() {
    let mut data = runtime();
    data["analysis"]["updated_at_ms"] = json!(1);
    data["analysis"]["momentary_lufs"] = json!(-99.7);
    let text = render_with_runtime(Workspace::Now, Overlay::None, 140, 40, data, |_| {});
    assert!(!text.contains("-99.7"));
    assert!(text.contains("LUFS Unavailable"));
}

#[test]
fn saved_and_applied_revision_states_are_visible_on_the_default_surface() {
    let mut data = runtime();
    data["requested_music_revision"] = json!(8);
    let text = render_with_runtime(Workspace::Now, Overlay::None, 140, 40, data.clone(), |_| {});
    assert!(text.contains("Not applied yet"));
    data["applied_music_revision"] = json!(8);
    let text = render_with_runtime(Workspace::Now, Overlay::None, 140, 40, data.clone(), |_| {});
    assert!(text.contains("Applied"));
    data["updated_at_ms"] = json!(1);
    let text = render_with_runtime(Workspace::Now, Overlay::None, 140, 40, data, |_| {});
    assert!(text.contains("Waiting for audio status"));
    assert!(text.contains("STALE"));
    assert!(!text.contains("LIVE"));
}

#[test]
fn compact_monitor_never_fabricates_a_peak_or_advertises_removed_expand_key() {
    let text = render_with_runtime(
        Workspace::Now,
        Overlay::None,
        80,
        24,
        json!({"active":true,"updated_at_ms":maris::analysis::now_ms()}),
        |_| {},
    );
    assert!(text.contains("Unavailable"));
    assert!(!text.contains("-120.0 dBFS"));
    assert!(!text.contains("M ·"));
}

#[test]
fn medium_frequency_labels_do_not_collide() {
    let text = render(Workspace::Now, Overlay::None, 100, 32);
    assert!(text.contains("20k"));
    assert!(!text.contains("1020k"));
    assert!(!text.contains("10k20k"));
}

#[test]
fn calculated_eq_response_is_nonflat_until_bypassed() {
    let mut profile = Profile::preset("warm").unwrap();
    let on = tui_view::response(&profile, 48_000);
    assert!(on.iter().any(|gain| gain.abs() > 0.5));
    profile.bypass = true;
    let off = tui_view::response(&profile, 48_000);
    assert!(off.iter().all(|gain| gain.abs() < 1e-10));
}

#[test]
fn popup_stays_inside_small_and_large_terminals() {
    for area in [
        ratatui::layout::Rect::new(3, 5, 0, 0),
        ratatui::layout::Rect::new(3, 5, 1, 1),
        ratatui::layout::Rect::new(0, 0, 20, 5),
        ratatui::layout::Rect::new(0, 0, 50, 14),
        ratatui::layout::Rect::new(0, 0, 140, 50),
    ] {
        let lines = [ratatui::text::Line::from(
            "A bounded modal should wrap instead of overflowing the terminal.",
        )];
        let popup = tui_view::popup(area, &lines);
        assert!(popup.x >= area.x);
        assert!(popup.y >= area.y);
        assert!(popup.right() <= area.right());
        assert!(popup.bottom() <= area.bottom());
    }
}
