//! Offline UI review. Fixture audio is measured; draft state uses isolated temporary files only.
//! No capture, routing or user-state writes.
pub use maris::{analysis, i18n, theme, ui, visualizer};
#[cfg(target_os = "macos")]
#[path = "../../src/ui/desktop/monitor.rs"]
mod hud;

use maris::{
    audio::DeviceInfo,
    control_panel::{Overlay, Workspace},
    store::Snapshot,
    tui_view::{self, Console},
};
use ratatui::{
    backend::TestBackend,
    style::{Color, Modifier},
    Terminal,
};
use serde_json::{json, Value};

fn fixture() -> anyhow::Result<Value> {
    let rate = 48_000;
    let partials = [
        (55.0, 0.08),
        (110.0, 0.07),
        (330.0, 0.05),
        (880.0, 0.04),
        (2400.0, 0.026),
        (5800.0, 0.018),
        (11800.0, 0.01),
    ];
    let frames: Vec<[f32; 2]> = (0..rate * 2)
        .map(|index| {
            let phase = std::f64::consts::TAU * index as f64 / rate as f64;
            let left = partials
                .iter()
                .map(|(hz, amp)| (phase * hz).sin() * amp)
                .sum::<f64>() as f32;
            let right = partials
                .iter()
                .map(|(hz, amp)| (phase * hz + 0.18).sin() * amp * 0.9)
                .sum::<f64>() as f32;
            [left, right]
        })
        .collect();
    let evidence = analysis::measure(&frames, rate)?;
    let mut analyzer = visualizer::Analyzer::new(rate);
    let mut spectrum = None;
    for frame in &frames {
        if let Some(value) = analyzer.push(*frame) {
            spectrum = Some(value);
        }
    }
    let spectrum = spectrum.ok_or_else(|| anyhow::anyhow!("Fixture spectrum did not complete"))?;
    let bands = spectrum
        .bands_dbfs
        .map(|db| ((db + 72.0) / 72.0).clamp(0.0, 1.0));
    let peak = |channel: usize| {
        20.0 * f64::from(
            frames
                .iter()
                .map(|f| f[channel].abs())
                .fold(0.0_f32, f32::max),
        )
        .max(1e-12)
        .log10()
    };
    Ok(json!({
        "ui_fixture":true, "active":true, "output":"Studio Headphones", "sample_rate":rate,
        "output_mode":"follow_system_default", "updated_at_ms":analysis::now_ms(),
        "profile_key":"Studio Headphones", "profile_binding_source":"stable_id",
        "device_capability":{"device_class":"headphone", "model":null, "bass_floor_hz":null, "treble_ceiling_hz":null},
        "peak_left_dbfs":peak(0), "peak_right_dbfs":peak(1), "effective_preamp_db":-2.5,
        "requested_music_revision":0,"applied_music_revision":0,"applied_revision":0,
        "analysis":evidence, "music_context":maris::music_context::MusicContext::signal_only(&evidence),
        "visualization":spectrum, "display_bands":bands,
        "mixer_strip_count":0, "mixer_control":{"config":{"strips":[], "buses":[{"output_device":null},{"output_device":null}]}},
        "models":maris::neural::models()
    }))
}

fn rgb(color: Color, background: bool) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Black => [0, 0, 0],
        Color::White => [255, 255, 255],
        Color::Gray => [170, 170, 170],
        Color::DarkGray => [85, 85, 85],
        Color::Red | Color::LightRed => [255, 85, 85],
        Color::Green | Color::LightGreen => [85, 255, 85],
        Color::Blue | Color::LightBlue => [85, 85, 255],
        Color::Yellow | Color::LightYellow => [255, 255, 85],
        Color::Cyan | Color::LightCyan => [85, 255, 255],
        Color::Magenta | Color::LightMagenta => [255, 85, 255],
        Color::Indexed(n) if n >= 232 => [8 + (n - 232) * 10; 3],
        Color::Indexed(n) if n >= 16 => {
            let n = n - 16;
            let component = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            [component(n / 36), component(n / 6 % 6), component(n % 6)]
        }
        _ => {
            if background {
                [7, 9, 13]
            } else {
                [232, 235, 242]
            }
        }
    }
}

fn render(
    width: u16,
    height: u16,
    active: bool,
    overlay: Overlay,
    workspace: Workspace,
    runtime_fixture: &Value,
) -> anyhow::Result<()> {
    let snapshot = Snapshot {
        profile: maris::profile::Profile::preset("warm")?,
        ..Snapshot::default()
    };
    let mut runtime = if active {
        runtime_fixture.clone()
    } else {
        json!({"active":false, "ui_fixture":true})
    };
    runtime["updated_at_ms"] = json!(analysis::now_ms());
    if active {
        runtime["visualization"]["updated_at_ms"] = json!(analysis::now_ms());
    }
    let devices = vec![DeviceInfo {
        id: "uid:FixtureHeadphones".into(),
        name: "Studio Headphones".into(),
        direction: "output".into(),
        is_default: true,
    }];
    let applications = json!({"ui_fixture":true,"available":true,"applications":[
        {"pid":111,"bundle_id":"Offline fixture / Music Player","running_output":true,"is_maris":false},
        {"pid":222,"bundle_id":"Offline fixture / Browser","running_output":false,"is_maris":false},
        {"pid":333,"bundle_id":"Offline fixture / Voice Call","running_output":false,"is_maris":false}
    ]});
    let presets = maris::presets::console_catalog();
    let models = maris::neural::models();
    let music = maris::music::MusicProfile::default();
    let palette = theme::palette(&runtime);
    let temporary = tempfile::tempdir()?;
    let state = maris::store::Store::at(temporary.path());
    let library = maris::listening::Library::default();
    let mut editor = maris::configuration::Editor::default();
    let playback_mode = std::env::args().any(|argument| argument == "--playback");
    let device_mode = std::env::args().any(|argument| argument == "--device-sound");
    let draft_mode = std::env::args().any(|argument| argument == "--draft")
        && workspace == Workspace::Sound
        && active;
    if draft_mode {
        runtime["session_id"] = json!("offline-settings-probe");
        runtime["device_identity"] = json!({"stable_id":"FixtureHeadphones"});
        runtime["music_processing"] = json!(true);
        runtime["rebind_count"] = json!(0);
        runtime["device_capability"] = serde_json::to_value(maris::device_profile::effective(
            &state,
            "Studio Headphones",
        )?)?;
        state.write_json("profile.json", &snapshot)?;
        state.write_json("runtime.json", &runtime)?;
        use crossterm::event::KeyCode::Char;
        let changes = if playback_mode {
            vec![(11, Char('+'))]
        } else if device_mode {
            vec![(0, Char('+')), (3, Char('-'))]
        } else {
            vec![
                (18, Char('-')),
                (18, Char('-')),
                (18, Char(']')),
                (23, Char('-')),
            ]
        };
        for (mut selected, key) in changes {
            editor.handle(
                &state,
                maris::configuration::Context {
                    runtime: &runtime,
                    eq: &snapshot,
                    listening: &library,
                },
                &mut selected,
                key,
            )?;
        }
        assert_eq!(state.load()?.revision, snapshot.revision);
        assert_eq!(state.load()?.profile, snapshot.profile);
        assert!(!state.directory.join("listening.json").exists());
    }
    // Replay the measured fixture as a current observation for every locale/size.
    // Slow offline exports must not accidentally become stale-data illustrations.
    let now = analysis::now_ms();
    runtime["updated_at_ms"] = json!(now);
    if active {
        runtime["visualization"]["updated_at_ms"] = json!(now);
        runtime["analysis"]["updated_at_ms"] = json!(now);
    }
    let view = Console {
        snapshot: &snapshot,
        configuration: Some(&editor),
        runtime: &runtime,
        workspace,
        sound_row: if workspace == Workspace::Sound {
            if playback_mode {
                11
            } else if device_mode {
                0
            } else {
                18
            }
        } else {
            0
        },
        home_row: 0,
        sound_scroll: 0,
        app_row: 0,
        pending_apps: &[111],
        devices: &devices,
        applications: &applications,
        selected_output: devices.first(),
        output_choice: 1,
        presets: &presets,
        preset_choice: 0,
        models: &models,
        music: &music,
        palette,
        notice: "OFFLINE UI FIXTURE — generated audio, not a hardware session",
        goal: "balanced",
        proposal: None,
        overlay,
    };
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| tui_view::draw(frame, &view))?;
    let state = if overlay != Overlay::None {
        "picker"
    } else if draft_mode {
        "draft"
    } else if active {
        "signal"
    } else {
        "idle"
    };
    let prefix = if workspace == Workspace::Now {
        "studio"
    } else {
        match workspace {
            Workspace::Sound if playback_mode => "settings-playback",
            Workspace::Sound if device_mode => "settings-device",
            Workspace::Sound => "inspector-eq",
            Workspace::Apps => "inspector-apps",
            Workspace::Device => "inspector-device",
            Workspace::Intelligence => "inspector-ai",
            _ => "inspector-health",
        }
    };
    let language = i18n::code();
    let suffix = if language == "en" {
        String::new()
    } else {
        format!("-{language}")
    };
    let stem = format!(".maris-review/{prefix}-{width}x{height}-{state}{suffix}");
    let cells = terminal.backend().buffer().content.iter().enumerate().map(|(index,cell)| json!({
        "x":index % width as usize, "y":index / width as usize, "text":cell.symbol(),
        "display_width":ratatui::text::Line::from(cell.symbol()).width(),
        "fg":rgb(cell.fg,false), "bg":rgb(cell.bg,true), "bold":cell.modifier.contains(Modifier::BOLD)
    })).collect::<Vec<_>>();
    let text = terminal
        .backend()
        .buffer()
        .content
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(format!("{stem}.txt"), &text)?;
    std::fs::write(
        format!("{stem}.json"),
        serde_json::to_vec(&json!({
            "width":width,"height":height,"label":"Offline Ratatui render / no hardware session","cells":cells
        }))?,
    )?;
    if width == 140 && active && overlay == Overlay::None && workspace == Workspace::Now {
        std::fs::write(".maris-review/console.txt", text)?;
    }
    println!("Rendered {stem}.json");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all(".maris-review")?;
    if std::env::args().any(|argument| argument == "--hud") {
        #[cfg(target_os = "macos")]
        return native_probe();
        #[cfg(not(target_os = "macos"))]
        anyhow::bail!("Native compact monitor probe is macOS-only");
    }
    let measured = fixture()?;
    if std::env::args().any(|argument| argument == "--settings") {
        let directory = tempfile::tempdir()?;
        let store = maris::store::Store::at(directory.path());
        for language in i18n::LANGUAGES {
            i18n::configure(&store, Some(language))?;
            for (width, height) in [(80, 24), (100, 32), (140, 40), (180, 50)] {
                render(
                    width,
                    height,
                    true,
                    Overlay::None,
                    Workspace::Sound,
                    &measured,
                )?;
            }
        }
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--locales") {
        let directory = tempfile::tempdir()?;
        let store = maris::store::Store::at(directory.path());
        for language in i18n::LANGUAGES {
            i18n::configure(&store, Some(language))?;
            for workspace in Workspace::ALL {
                for (width, height) in [(100, 32), (140, 40)] {
                    render(width, height, true, Overlay::None, workspace, &measured)?;
                }
            }
            render(140, 40, false, Overlay::None, Workspace::Now, &measured)?;
            render(140, 40, true, Overlay::Preset, Workspace::Now, &measured)?;
        }
        return Ok(());
    }
    for (width, height) in [(80, 24), (100, 32), (120, 36), (140, 40), (180, 50)] {
        render(
            width,
            height,
            true,
            Overlay::None,
            Workspace::Now,
            &measured,
        )?;
    }
    render(140, 40, false, Overlay::None, Workspace::Now, &measured)?;
    render(100, 32, true, Overlay::Output, Workspace::Now, &measured)?;
    for detail in Workspace::DETAILS {
        for (width, height) in [(100, 32), (140, 40)] {
            render(width, height, true, Overlay::None, detail, &measured)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_probe() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::run_on_demand::EventLoopExtRunOnDemand,
        window::WindowId,
    };
    let mut events = EventLoop::new()?;
    let monitor = hud::Hud::new(true)?;
    let _window_number = monitor.window_number();
    monitor.toggle();
    monitor.toggle();
    let mut runtime = fixture()?;
    runtime["output"] = json!("OFFLINE UI FIXTURE");
    let idle = std::env::args().any(|argument| argument == "--idle");
    let stale = std::env::args().any(|argument| argument == "--stale");
    if std::env::args().any(|argument| argument == "--missing-peak") {
        runtime.as_object_mut().unwrap().remove("peak_dbfs");
    }
    if idle {
        runtime["active"] = json!(false);
    }
    struct Probe {
        monitor: hud::Hud,
        runtime: Value,
        stale: bool,
        began: Instant,
    }
    impl ApplicationHandler for Probe {
        fn resumed(&mut self, _: &ActiveEventLoop) {}
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        fn about_to_wait(&mut self, events: &ActiveEventLoop) {
            let now = analysis::now_ms();
            self.runtime["updated_at_ms"] = json!(if self.stale {
                now.saturating_sub(2000)
            } else {
                now
            });
            self.runtime["visualization"]["updated_at_ms"] = json!(now);
            self.monitor
                .tick(&self.runtime, self.runtime["tonal_bypass"] == true);
            if self.began.elapsed() > Duration::from_secs(2) {
                events.exit();
            } else {
                events.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(33),
                ));
            }
        }
    }
    events.run_app_on_demand(&mut Probe {
        monitor,
        runtime,
        stale,
        began: Instant::now(),
    })?;
    println!(
        "offline_compact_monitor_event_loop_completed / generated audio / no hardware session"
    );
    Ok(())
}
