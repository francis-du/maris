use maris::{
    control_panel::{Overlay, Workspace},
    store::Snapshot,
    studio_view::{
        self,
        appearance::{Appearance, ColorDepth},
    },
    tui_view::{self, Console},
};
use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    Terminal,
};
use serde_json::{json, Value};

fn render(runtime: &Value, workspace: Workspace, overlay: Overlay, size: (u16, u16)) -> Buffer {
    let snapshot = Snapshot::default();
    let music = maris::music::MusicProfile::default();
    let view = Console {
        snapshot: &snapshot,
        configuration: None,
        runtime,
        workspace,
        sound_row: 16,
        home_row: 0,
        sound_scroll: 0,
        app_row: 0,
        pending_apps: &[],
        devices: &[],
        applications: &json!({"available":false}),
        selected_output: None,
        output_choice: 0,
        presets: &maris::presets::console_catalog(),
        preset_choice: 0,
        models: &json!({}),
        music: &music,
        palette: maris::theme::VIOLET,
        notice: "Offline appearance regression",
        goal: "balanced",
        proposal: None,
        overlay,
    };
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| tui_view::draw(frame, &view)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

fn plot(buffer: &Buffer) -> Vec<String> {
    let area = studio_view::layout(maris::studio_controls::shell(buffer.area).content).analyzer;
    let area = Rect::new(
        area.x + 5,
        area.y + 2,
        area.width.saturating_sub(27),
        area.height.saturating_sub(6),
    );
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| buffer[(x, y)].symbol().to_owned()))
        .collect()
}

#[test]
fn real_console_preserves_operations_when_colors_or_motion_are_reduced() {
    // Each child owns its environment; parallel tests never mutate process-wide UI settings.
    if std::env::var_os("MARIS_APPEARANCE_CHILD").is_none() {
        for (term, colorterm, no_color, reduced) in [
            ("xterm-256color", "truecolor", "", "0"),
            ("xterm-256color", "", "", "0"),
            ("xterm", "", "", "0"),
            ("xterm-256color", "truecolor", "", "1"),
            ("xterm-256color", "truecolor", "1", "0"),
            ("xterm-256color", "truecolor", "1", "1"),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "real_console_preserves_operations_when_colors_or_motion_are_reduced",
                    "--nocapture",
                ])
                .env("MARIS_APPEARANCE_CHILD", "1")
                .env("TERM", term)
                .env("COLORTERM", colorterm)
                .env("NO_COLOR", no_color)
                .env("MARIS_REDUCED_MOTION", reduced)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{term}/{no_color}/{reduced}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }

    let now = maris::analysis::now_ms();
    let mut analyzer = maris::visualizer::Analyzer::new(48_000);
    let mut measured = None;
    for index in 0..12_000 {
        let sample = (index as f64 * std::f64::consts::TAU * 997.0 / 48_000.0).sin() as f32 * 0.1;
        measured = analyzer.push([sample; 2]).or(measured);
    }
    let mut runtime = json!({"active":true,"updated_at_ms":now,"sample_rate":48000,
        "visualization":measured.unwrap(),"peak_left_dbfs":-12.3,"peak_right_dbfs":-16.7});
    runtime["visualization"]["updated_at_ms"] = json!(now);
    let mut motion = maris::visualizer::Motion::default();
    motion.update(&runtime, 0.033, now);
    runtime["display_bands"] = json!(motion.bands);
    let appearance = Appearance::from_environment();
    for workspace in Workspace::ALL {
        for overlay in [
            Overlay::None,
            Overlay::Help,
            Overlay::Output,
            Overlay::Preset,
        ] {
            for size in [(54, 14), (100, 32), (140, 40)] {
                let fresh = maris::analysis::now_ms();
                runtime["updated_at_ms"] = json!(fresh);
                runtime["visualization"]["updated_at_ms"] = json!(fresh);
                let buffer = render(&runtime, workspace, overlay, size);
                for cell in &buffer.content {
                    match appearance.color_depth {
                        ColorDepth::None => {
                            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset))
                        }
                        ColorDepth::Basic => assert!(
                            !matches!(cell.fg, Color::Rgb(..) | Color::Indexed(_))
                                && !matches!(cell.bg, Color::Rgb(..) | Color::Indexed(_))
                        ),
                        ColorDepth::Indexed => assert!(
                            !matches!(cell.fg, Color::Rgb(..))
                                && !matches!(cell.bg, Color::Rgb(..))
                        ),
                        ColorDepth::TrueColor => {}
                    }
                }
                if overlay == Overlay::None {
                    assert!(text(&buffer).contains("MARIS"), "{workspace:?} {size:?}");
                }
            }
        }
    }
    runtime["updated_at_ms"] = json!(maris::analysis::now_ms());
    runtime["visualization"]["updated_at_ms"] = runtime["updated_at_ms"].clone();
    let first = render(&runtime, Workspace::Now, Overlay::None, (140, 40));
    runtime["display_bands"] = json!(vec![0.9; 24]);
    let second = render(&runtime, Workspace::Now, Overlay::None, (140, 40));
    if appearance.reduced_motion {
        assert_eq!(
            plot(&first),
            plot(&second),
            "moving spectrum remains in reduced-motion mode"
        );
        assert!(text(&first).contains("Reduced motion"));
    } else {
        assert_ne!(
            plot(&first),
            plot(&second),
            "real signal changes must remain visible"
        );
    }
    assert!(
        text(&second).contains("-12.3") && text(&second).contains("-16.7"),
        "numeric measurements were hidden with motion"
    );
    for size in [(54, 14), (140, 40)] {
        runtime["updated_at_ms"] = json!(now.saturating_sub(2000));
        let stale = render(&runtime, Workspace::Now, Overlay::None, size);
        assert!(
            text(&stale).contains("STALE"),
            "stale audio looked like standby: {size:?}"
        );
        assert!(
            !text(&stale).contains("-12.3"),
            "stale peak remained visible"
        );
    }
}

#[test]
fn capability_detection_keeps_color_opt_out_and_supported_depth() {
    assert_eq!(
        Appearance::detect(Some("xterm-256color"), Some("truecolor"), true),
        ColorDepth::None
    );
    assert_eq!(
        Appearance::detect(Some("dumb"), Some("truecolor"), false),
        ColorDepth::None
    );
    assert_eq!(
        Appearance::detect(Some("xterm-256color"), None, false),
        ColorDepth::Indexed
    );
    assert_eq!(
        Appearance::detect(Some("vt100"), None, false),
        ColorDepth::Basic
    );
    assert_eq!(
        ColorDepth::Indexed.map(Color::Rgb(95, 135, 175)),
        Color::Indexed(67)
    );
    assert_eq!(
        ColorDepth::Indexed.map(Color::Rgb(128, 128, 128)),
        Color::Indexed(244)
    );
    assert_eq!(
        ColorDepth::Basic.map(Color::Rgb(255, 0, 0)),
        Color::LightRed
    );
    let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
    buffer[(0, 0)].set_symbol("L").set_fg(maris::theme::MUTED);
    buffer[(1, 0)].set_symbol("V").set_fg(maris::theme::TEXT);
    Appearance {
        color_depth: ColorDepth::None,
        reduced_motion: false,
    }
    .apply(&mut buffer);
    assert_eq!(
        (buffer[(0, 0)].fg, buffer[(1, 0)].fg),
        (Color::Reset, Color::Reset)
    );
    assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
    assert!(!buffer[(1, 0)].modifier.contains(Modifier::DIM));
    assert_eq!(
        (buffer[(0, 0)].symbol(), buffer[(1, 0)].symbol()),
        ("L", "V")
    );
}
