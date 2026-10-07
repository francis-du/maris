pub mod mixer;
pub mod sound;

use crate::{
    analysis, audio,
    audio::offline,
    control,
    control::mcp,
    control::store::{self, Store},
    dsp::profile::Profile,
    models as neural, presets,
    tuning::eq as smart,
    ui::desktop,
    ui::tui,
};
use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "maris",
    version,
    about = "Adjust system audio, save device settings, and mix apps or inputs."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "UI language: en, zh-CN, zh-TW, ja, de, es"
    )]
    /// Interface language code.
    lang: Option<String>,
    #[arg(long, global = true, help = "Emit structured JSON, including errors")]
    /// Emit structured JSON, including errors.
    json: bool,
    #[arg(
        long,
        global = true,
        help = "Do not automatically launch the menu-bar/system-tray process"
    )]
    /// Do not launch the tray controller.
    no_tray: bool,
    #[arg(
        long,
        global = true,
        help = "Apply only if the saved settings still have this revision"
    )]
    /// Apply only at this saved settings revision.
    expected_revision: Option<u64>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum PresetCatalogAction {
    /// List the included presets by category.
    List,
    /// Show source data, converted Q values, and safety headroom for one preset.
    Show {
        /// Preset name or stable ID.
        name: String,
    },
    /// Apply an included preset.
    Apply {
        /// Preset name or stable ID.
        name: String,
    },
    /// Restore the settings from before the last change.
    Restore,
}

#[derive(Subcommand)]
enum ModelAction {
    /// Show bundled model versions, checksums, and sources.
    Research,
}

#[derive(Subcommand)]
enum Command {
    #[cfg(unix)]
    #[command(name = "__route-watch", hide = true)]
    RouteWatch { token: String },
    /// Open Maris and automatically tune system playback after OS authorization.
    App,
    /// Build a local macOS application bundle from this executable.
    Package,
    /// Replace this installed release with the latest stable release, or a pinned version.
    Update {
        #[arg(long, help = "Install a specific stable X.Y.Z or vX.Y.Z release")]
        /// Stable release version to install.
        version: Option<String>,
        #[arg(
            long,
            help = "Show the installer plan without network access or file changes"
        )]
        /// Validate or preview without applying changes.
        dry_run: bool,
    },
    /// Show or change the interface language.
    Language {
        /// Interface language code; omit to show the current language.
        code: Option<String>,
    },
    /// Read or change the current device's sound settings.
    Sound {
        #[arg(long)]
        /// Saved device profile key; omit to use the active output.
        device: Option<String>,
        #[command(subcommand)]
        action: crate::cli::sound::Action,
    },
    /// Configure separate app/input channels and up to two outputs.
    Mixer {
        #[command(subcommand)]
        action: crate::cli::mixer::Action,
    },
    /// List available audio devices and stable platform identifiers.
    Devices,
    /// List apps with audio without changing their playback.
    Applications,
    /// Process selected apps without changing the system default output.
    Application {
        #[arg(long = "pid", required = true, num_args = 1..)]
        /// Application process IDs to capture.
        pids: Vec<i32>,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Authorize temporary audio capture and routing.
        accept_routing: bool,
        #[arg(long, help = "Stop after this many seconds")]
        /// Stop automatically after this many seconds.
        seconds: Option<u64>,
    },
    /// Show which local audio-analysis models are included.
    Models {
        #[command(subcommand)]
        action: Option<ModelAction>,
    },
    /// Analyze an audio file and print measured signal/music context.
    Analyze {
        /// Input file path.
        file: PathBuf,
    },
    /// Create a bounded tuning proposal from live or file analysis.
    Smart {
        #[arg(long, default_value = "balanced")]
        /// Listening goal for the tuning proposal.
        goal: String,
        #[arg(long)]
        /// Input file path.
        file: Option<PathBuf>,
        #[arg(long)]
        /// Apply the result after validation.
        apply: bool,
        #[arg(long)]
        /// Write the proposal to this file.
        save: Option<PathBuf>,
    },
    /// Apply a previously saved Smart proposal after revision checks.
    SmartApply {
        /// Saved tuning proposal JSON file.
        file: PathBuf,
    },
    /// Run speech-only RNNoise enhancement on a file.
    Enhance {
        /// Input audio file path.
        input: PathBuf,
        /// Output audio file path; existing files are preserved.
        output: PathBuf,
        #[arg(long)]
        /// Confirm that this input is speech, not music.
        confirm_speech: bool,
    },
    /// Run live speech processing from one input to an output.
    Voice {
        #[arg(long)]
        /// Input device name or stable identifier.
        input: String,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Confirm that this input is speech, not music.
        confirm_speech: bool,
    },
    /// Start a detached live/system audio session through the desktop controller.
    Start {
        #[arg(long)]
        /// Input device name or stable identifier.
        input: Option<String>,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Capture system playback instead of an input device.
        system: bool,
        #[arg(long)]
        /// Authorize temporary audio capture and routing.
        accept_routing: bool,
    },
    /// Request the currently running Maris audio session to stop.
    Stop,
    /// Run or ensure the native menu-bar/system-tray controller.
    Tray {
        #[arg(long)]
        /// Run the tray controller in the background.
        background: bool,
    },
    /// Inspect native audio capabilities, permissions, devices, and recovery state.
    Doctor,
    /// Print saved settings and current runtime telemetry.
    Status,
    /// List, inspect, apply, or restore global EQ presets.
    Presets {
        #[command(subcommand)]
        action: Option<PresetCatalogAction>,
    },
    /// Compatibility alias for `maris presets apply <name>`.
    Preset {
        /// Preset name or stable ID.
        name: String,
    },
    /// Edit one global EQ band.
    Band {
        /// EQ band number, from 1 through 10.
        index: usize,
        #[arg(allow_hyphen_values = true)]
        /// EQ band gain in dB.
        gain_db: f64,
        #[arg(long)]
        /// EQ band center frequency in Hz.
        frequency: Option<f64>,
        #[arg(long)]
        /// EQ band quality factor.
        q: Option<f64>,
    },
    /// Set the global preamp in dB.
    Preamp {
        #[arg(allow_hyphen_values = true)]
        /// Preamp gain in dB.
        db: f64,
    },
    /// Enable or disable global processing bypass while retaining safety gain.
    Bypass {
        #[arg(action = clap::ArgAction::Set)]
        /// Whether to bypass tonal processing: true or false.
        value: bool,
    },
    /// Set global headphone crossfeed amount.
    Crossfeed {
        /// Effect amount.
        amount: f64,
    },
    /// Set global stereo-width amount.
    Width {
        /// Effect amount.
        amount: f64,
    },
    /// Undo the most recent compatible global settings change.
    Undo,
    /// Print the JSON schema for the global DSP profile.
    Schema,
    /// Validate or apply a global DSP profile JSON file.
    Apply {
        /// Settings JSON file to validate or apply.
        file: PathBuf,
        #[arg(long)]
        /// Validate or preview without applying changes.
        dry_run: bool,
    },
    /// Export the current global DSP profile to a new JSON file.
    Export {
        /// New JSON file for exported settings.
        file: PathBuf,
    },
    /// Render an audio file offline through the current DSP profile.
    Render {
        /// Input audio file path.
        input: PathBuf,
        /// Output audio file path; existing files are preserved.
        output: PathBuf,
    },
    /// Process a live input device until stopped or the optional timeout expires.
    Run {
        #[arg(long)]
        /// Input device name or stable identifier.
        input: String,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Stop automatically after this many seconds.
        seconds: Option<u64>,
    },
    /// Play an audio file through Maris processing.
    Play {
        /// Input file path.
        file: PathBuf,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Repeat the file until stopped.
        repeat: bool,
    },
    /// Open the terminal UI, attaching to or starting an audio session as needed.
    Tui {
        #[arg(long)]
        /// Input device name or stable identifier.
        input: Option<String>,
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Audio file to play in the terminal UI.
        play: Option<PathBuf>,
    },
    /// Process macOS/Windows/Linux system playback with explicit routing authorization.
    System {
        #[arg(long)]
        /// Output device name or stable identifier; omit to use the default.
        output: Option<String>,
        #[arg(long)]
        /// Authorize temporary audio capture and routing.
        accept_routing: bool,
        #[arg(long, help = "Stop after this many seconds")]
        /// Stop automatically after this many seconds.
        seconds: Option<u64>,
    },
    /// Restore platform audio routing owned by a previous Maris session.
    Restore,
    /// Serve the local MCP control protocol; writes are opt-in.
    Mcp {
        #[arg(long)]
        /// Authorize validated MCP commands to change settings.
        allow_write: bool,
    },
    /// Print MCP integration configuration for a supported client.
    Integration {
        /// MCP client to generate integration settings for.
        client: String,
    },
}

pub fn main() {
    let cli = Cli::parse();
    let json_output = cli.json;
    if let Err(error) = run(cli) {
        if json_output {
            let code = if error
                .downcast_ref::<crate::presets::PresetNotFound>()
                .is_some()
            {
                "PRESET_NOT_FOUND"
            } else {
                "MARIS_ERROR"
            };
            println!(
                "{}",
                json!({"ok":false,"error":{"code":code,"message":format!("{error:#}")}})
            );
        } else {
            eprintln!("Maris: {error:#}");
        }
        std::process::exit(1);
    }
}
fn print(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
fn run(cli: Cli) -> Result<()> {
    let store = Store::discover()?;
    crate::i18n::configure(&store, cli.lang.as_deref())?;
    let revision = cli.expected_revision;
    let no_tray = cli.no_tray;
    match cli.command.unwrap_or(Command::App) {
        #[cfg(unix)]
        Command::RouteWatch { token } => audio::watch_route(&store, &token),
        Command::App => {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                tui::run(store, None, None, None, no_tray)
            } else {
                ensure!(
                    !no_tray,
                    "Use the explicit system command for a headless session"
                );
                desktop::run_app(store)
            }
        }
        Command::Package => print(desktop::package_macos()?),
        Command::Update { version, dry_run } => update(version.as_deref(), dry_run),
        Command::Language { code } => {
            if let Some(code) = code {
                crate::i18n::save(&store, &code)?;
            }
            print(json!({"language":crate::i18n::code(),"supported":crate::i18n::LANGUAGES}))
        }
        Command::Sound { device, action } => {
            print(crate::cli::sound::run(&store, device, revision, action)?)
        }
        Command::Mixer { action } => print(crate::cli::mixer::run(&store, revision, action)?),
        Command::Devices => print(audio::devices()?),
        Command::Applications => print(audio::applications()?),
        Command::Application {
            pids,
            output,
            accept_routing,
            seconds,
        } => {
            ensure!(
                accept_routing,
                "Application audio processing temporarily replaces selected playback routes; add --accept-routing to authorize this session"
            );
            store.write_json(
                "startup.json",
                &json!({
                    "phase":"requesting_application_audio",
                    "pids":pids,
                    "pid":std::process::id(),
                    "updated_at_ms":analysis::now_ms()
                }),
            )?;
            let result = audio::Session::application_audio(store.clone(), &pids, output.as_deref());
            match result {
                Ok(session) => {
                    store.write_json(
                        "startup.json",
                        &json!({
                            "phase":"connected_application_audio",
                            "pids":pids,
                            "pid":std::process::id(),
                            "updated_at_ms":analysis::now_ms()
                        }),
                    )?;
                    wait(session, seconds, no_tray)
                }
                Err(error) => {
                    store.write_json(
                        "startup.json",
                        &json!({
                            "phase":"failed_application_audio",
                            "pids":pids,
                            "pid":std::process::id(),
                            "error":format!("{error:#}"),
                            "updated_at_ms":analysis::now_ms()
                        }),
                    )?;
                    Err(error)
                }
            }
        }
        Command::Models { action } => match action {
            None => print(neural::models_at(&store)?),
            Some(ModelAction::Research) => print(neural::research_models_at(&store)?),
        },
        Command::Analyze { file } => print(analysis::analyze_file(&file)?),
        Command::Smart {
            goal,
            file,
            apply,
            save,
        } => {
            let plan = if let Some(file) = file {
                smart::propose(&store.load()?, &analysis::analyze_file(&file)?, &goal)?
            } else {
                smart::from_live(&store, &goal)?
            };
            if let Some(expected) = revision {
                ensure!(plan.baseline_revision == expected, "Revision conflict");
            }
            if let Some(file) = save {
                store::atomic_json_new(&file, &plan)?;
            }
            if apply {
                print(json!({"proposal":plan,"state":smart::apply(&store, &plan)?}))
            } else {
                print(plan)
            }
        }
        Command::SmartApply { file } => {
            let plan: smart::Proposal = store::read_json(&file)?;
            if let Some(expected) = revision {
                ensure!(plan.baseline_revision == expected, "Revision conflict");
            }
            print(smart::apply(&store, &plan)?)
        }
        Command::Enhance {
            input,
            output,
            confirm_speech,
        } => {
            ensure!(confirm_speech, "RNNoise is speech-only and can damage music. Add --confirm-speech to authorize this operation");
            print(neural::enhance(&input, &output)?)
        }
        Command::Voice {
            input,
            output,
            confirm_speech,
        } => {
            ensure!(confirm_speech, "RNNoise is speech-only. Use --confirm-speech and physical headphones at a low volume");
            wait(
                audio::Session::live_voice(store, &input, output.as_deref())?,
                None,
                no_tray,
            )
        }
        Command::Start {
            input,
            output,
            system,
            accept_routing,
        } => {
            ensure!(
                system != input.is_some(),
                "Specify exactly one of --system or --input"
            );
            ensure!(
                !system || accept_routing,
                "System routing requires --accept-routing"
            );
            if !no_tray {
                desktop::ensure_running(&store)?;
            }
            let mut args = vec![if system {
                "system".into()
            } else {
                "run".into()
            }];
            if let Some(input) = input {
                args.extend(["--input".into(), input]);
            }
            if let Some(output) = output {
                args.extend(["--output".into(), output]);
            }
            if system {
                args.push("--accept-routing".into());
            }
            print(desktop::launch_audio(&store, &args)?)
        }
        Command::Stop => {
            control::request_stop(&store)?;
            print(json!({"stop_requested":true}))
        }
        Command::Tray { background } => {
            if background {
                desktop::ensure_running(&store)?;
                print(json!({"desktop_running":true,"capture_started":false}))
            } else {
                desktop::run(store)
            }
        }
        Command::Doctor => print(audio::doctor(&store)?),
        Command::Status => print(
            json!({"state":store.load()?,"runtime":audio::runtime_status(&store),"state_directory":store.directory}),
        ),
        Command::Presets { action } => {
            let runtime = audio::runtime_status(&store);
            let rate = runtime["sample_rate"]
                .as_u64()
                .filter(|rate| (44100..=192000).contains(rate))
                .unwrap_or(48000) as u32;
            match action.unwrap_or(PresetCatalogAction::List) {
                PresetCatalogAction::List => {
                    let catalog = presets::list();
                    print(json!({"count":catalog.len(),"presets":catalog}))
                }
                PresetCatalogAction::Show { name } => print(presets::show(&name, rate)?),
                PresetCatalogAction::Apply { name } => {
                    let details = presets::show(&name, rate)?;
                    let state = store.edit(revision, |profile| {
                        *profile = presets::apply_tone_curve(profile, &name, rate)?;
                        Ok(())
                    })?;
                    print(json!({"preset":details,"state":state}))
                }
                PresetCatalogAction::Restore => print(store.undo(revision)?),
            }
        }
        Command::Preset { name } => {
            let runtime = audio::runtime_status(&store);
            let rate = runtime["sample_rate"]
                .as_u64()
                .filter(|rate| (44100..=192000).contains(rate))
                .unwrap_or(48000) as u32;
            print(store.edit(revision, |profile| {
                *profile = presets::apply_tone_curve(profile, &name, rate)?;
                Ok(())
            })?)
        }
        Command::Band {
            index,
            gain_db,
            frequency,
            q,
        } => print(store.edit(revision, |p| {
            ensure!((1..=10).contains(&index), "Band index must be 1..10");
            let b = &mut p.bands[index - 1];
            b.gain_db = gain_db;
            if let Some(f) = frequency {
                b.frequency_hz = f;
            }
            if let Some(q) = q {
                b.q = q;
                b.bandwidth_octaves = None;
            }
            p.name = "custom".into();
            Ok(())
        })?),
        Command::Preamp { db } => print(store.edit(revision, |p| {
            p.preamp_db = db;
            Ok(())
        })?),
        Command::Bypass { value } => print(store.edit(revision, |p| {
            p.bypass = value;
            Ok(())
        })?),
        Command::Crossfeed { amount } => print(store.edit(revision, |p| {
            p.crossfeed = amount;
            Ok(())
        })?),
        Command::Width { amount } => print(store.edit(revision, |p| {
            p.stereo_width = amount;
            Ok(())
        })?),
        Command::Undo => print(store.undo(revision)?),
        Command::Schema => print(
            schemars::generate::SchemaSettings::draft07()
                .into_generator()
                .into_root_schema_for::<Profile>(),
        ),
        Command::Apply { file, dry_run } => {
            let p: Profile = store::read_json(&file)?;
            p.validate()?;
            if dry_run {
                print(
                    json!({"valid":true,"effective_preamp_db":p.effective_preamp_db(),"profile":p}),
                )
            } else {
                print(store.edit(revision, |profile| {
                    *profile = p;
                    Ok(())
                })?)
            }
        }
        Command::Export { file } => {
            ensure!(
                !file.exists(),
                "Export destination exists; choose a new path"
            );
            let p = store.load()?.profile;
            store::atomic_json_new(&file, &p)?;
            print(json!({"exported":file}))
        }
        Command::Render { input, output } => {
            print(offline::render(&input, &output, &store.load()?.profile)?)
        }
        Command::Run {
            input,
            output,
            seconds,
        } => wait(
            audio::Session::live(store, &input, output.as_deref())?,
            seconds,
            no_tray,
        ),
        Command::Play {
            file,
            output,
            repeat,
        } => wait(
            audio::Session::play(store, &file, output.as_deref(), repeat)?,
            None,
            no_tray,
        ),
        Command::Tui {
            input,
            output,
            play,
        } => tui::run(store, input, output, play, no_tray),
        Command::System {
            output,
            accept_routing,
            seconds,
        } => {
            ensure!(accept_routing, "Explicit CLI capture requires --accept-routing. Opening the Maris application requests OS authorization automatically");
            store.write_json("startup.json", &json!({"phase":"requesting_system_audio","pid":std::process::id(),"updated_at_ms":analysis::now_ms()}))?;
            let result = audio::Session::system(store.clone(), output.as_deref());
            match result {
                Ok(session) => {
                    store.write_json("startup.json", &json!({"phase":"connected","pid":std::process::id(),"updated_at_ms":analysis::now_ms()}))?;
                    wait(session, seconds, no_tray)
                }
                Err(error) => {
                    store.write_json("startup.json", &json!({"phase":"failed","pid":std::process::id(),"error":format!("{error:#}"),"updated_at_ms":analysis::now_ms()}))?;
                    Err(error)
                }
            }
        }
        Command::Restore => {
            audio::restore(&store)?;
            print(json!({"restored":true}))
        }
        Command::Mcp { allow_write } => mcp::serve(&store, allow_write),
        Command::Integration { client } => {
            ensure!(
                ["codex", "claude", "other"].contains(&client.as_str()),
                "Client must be codex, claude, or other"
            );
            let exe = std::env::current_exe()?.to_string_lossy().into_owned();
            if client == "codex" {
                println!(
                    "[mcp_servers.maris]\ncommand = {}\nargs = [\"mcp\", \"--allow-write\"]",
                    serde_json::to_string(&exe)?
                );
                Ok(())
            } else {
                print(
                    json!({"mcpServers":{"maris":{"command":exe,"args":["mcp","--allow-write"]}}}),
                )
            }
        }
    }
}

#[cfg(unix)]
fn update(version: Option<&str>, dry_run: bool) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let executable = std::env::current_exe()?.canonicalize()?;
    let package_root = executable
        .parent()
        .and_then(|path| path.parent())
        .context("Cannot resolve installed Maris package root")?;
    let lib = package_root
        .parent()
        .context("Cannot resolve installed Maris lib directory")?;
    ensure!(
        lib.file_name().and_then(|name| name.to_str()) == Some("lib"),
        "Update is available from an installed CLI release; reinstall Maris once with the public installer"
    );
    let prefix = lib
        .parent()
        .context("Cannot resolve Maris install prefix")?;
    let installer = package_root.join("resources/install.sh");
    ensure!(
        installer.is_file(),
        "This Maris release does not contain the updater; reinstall once with the public installer"
    );
    let mut command = std::process::Command::new("/bin/bash");
    command
        .arg(installer)
        .arg("--prefix")
        .arg(prefix)
        .arg("--yes");
    if let Some(version) = version {
        command.arg("--version").arg(version);
    }
    if dry_run {
        command.arg("--dry-run");
    }
    let error = command.exec();
    Err(error).context("Launch Maris updater")
}

#[cfg(windows)]
fn update(version: Option<&str>, dry_run: bool) -> Result<()> {
    // current_exe already returns the absolute module path. Canonicalizing it
    // adds a verbatim prefix that Windows PowerShell cannot use with -File.
    let executable = std::env::current_exe()?;
    let package_root = executable
        .parent()
        .and_then(|path| path.parent())
        .context("Cannot resolve installed Maris package root")?;
    let prefix = package_root
        .parent()
        .context("Cannot resolve Maris install prefix")?;
    let installer = package_root.join("resources/install.ps1");
    ensure!(
        installer.is_file(),
        "This Maris release does not contain the updater; reinstall once with the public installer"
    );
    let mut command = std::process::Command::new("powershell.exe");
    command
        .args(["-NoLogo", "-NoProfile", "-File"])
        .arg(installer)
        .arg("-Prefix")
        .arg(prefix)
        .arg("-Yes");
    if let Some(version) = version {
        command.arg("-Version").arg(version);
    }
    if dry_run {
        command.arg("-DryRun");
        let status = command.status()?;
        ensure!(status.success(), "Maris update dry run failed");
        return Ok(());
    }
    command
        .arg("-WaitForPid")
        .arg(std::process::id().to_string())
        .spawn()
        .context("Launch Maris updater")?;
    print(
        json!({"update_started":true,"current_version":env!("CARGO_PKG_VERSION"),"requested":version.unwrap_or("latest")}),
    )
}

fn wait(mut session: audio::Session, seconds: Option<u64>, no_tray: bool) -> Result<()> {
    if !no_tray {
        if let Err(error) = desktop::ensure_running(session.store()) {
            eprintln!("Maris tray unavailable: {error:#}. Audio remains in this terminal; Ctrl+C stops it.");
        }
    }
    let quit = Arc::new(AtomicBool::new(false));
    let signal = quit.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .context("Install shutdown handler")?;
    eprintln!("Maris is processing audio. Open 'maris tui' to tune; Ctrl+C stops this session.");
    let start = Instant::now();
    while !quit.load(Ordering::Relaxed) && !session.finished() {
        session.tick()?;
        if seconds.is_some_and(|s| start.elapsed() >= Duration::from_secs(s)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if seconds.is_some() {
        print(json!({"completed":true,"session":session.status()}))?;
    }
    Ok(())
}
