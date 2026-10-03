//! Session/routing helpers kept out of the main TUI event loop.
use crate::{
    audio::{self, DeviceInfo},
    control,
    control::store::Store,
    i18n::Notice,
    ui::{desktop, tui::input::{CursorMemory, Workspace}},
};
use anyhow::{ensure, Result};
use std::path::PathBuf;

use super::{actions::source_args, actions::captured_application_pids};

pub(super) fn sync_application_scope(
    runtime: &serde_json::Value,
    pending: &mut Vec<i32>,
    dirty: bool,
) {
    if !dirty {
        *pending = if runtime["active"] == true {
            captured_application_pids(runtime)
        } else {
            Vec::new()
        };
    }
}

pub(super) fn discard_application_draft(
    current: Workspace,
    destination: Workspace,
    runtime: &serde_json::Value,
    pending: &mut Vec<i32>,
    dirty: &mut bool,
) {
    if current == Workspace::Apps && destination != Workspace::Apps && *dirty {
        *pending = captured_application_pids(runtime);
        *dirty = false;
    }
}

pub(super) fn navigate(
    workspace: &mut Workspace,
    row: &mut usize,
    cursors: &mut CursorMemory,
    to: Workspace,
) {
    *row = cursors.visit(*workspace, to, *row);
    *workspace = to;
}

pub(super) fn start_initial_audio(
    store: &Store,
    play: Option<&PathBuf>,
    input_name: Option<&String>,
    input: Option<&DeviceInfo>,
    output: Option<&DeviceInfo>,
    notice: &mut Notice,
) -> Result<()> {
    if let Some(path) = play {
        let mut args = vec![
            "play".into(),
            path.canonicalize()?.to_string_lossy().into_owned(),
            "--repeat".into(),
        ];
        if let Some(output) = output {
            args.extend(["--output".into(), output.id.clone()]);
        }
        desktop::launch_audio(store, &args)?;
    } else if input_name.is_some() {
        desktop::launch_audio(store, &source_args("run", input, output))?;
    } else if control::is_stopped(store) {
        launch_system(store, output)?;
        *notice = "Starting native system audio.".into();
    }
    Ok(())
}

pub(super) fn launch_system(store: &Store, output: Option<&DeviceInfo>) -> Result<()> {
    let mut args = source_args("system", None, output);
    args.push("--accept-routing".into());
    desktop::launch_audio(store, &args).map(|_| ())
}

pub(super) fn apply_output_choice(
    store: &Store,
    runtime: &serde_json::Value,
    selected: Option<&DeviceInfo>,
    pin_output: &mut bool,
    output_index: &mut Option<usize>,
    outputs: &[DeviceInfo],
    notice: &mut Notice,
) -> Result<()> {
    if runtime["active"] == true && audio::native_controls(runtime) {
        control::request_output(store, selected.map(|device| device.id.as_str()))?;
        *notice = selected.map_or_else(
            || "Output request queued: follow system default.".into(),
            |device| Notice::new("Output request queued: {device}.").arg("device", &device.name),
        );
    } else if control::is_stopped(store) {
        launch_system(store, selected)?;
        *notice = selected.map_or_else(
            || "Starting on the system default output.".into(),
            |device| Notice::new("Starting on {device}.").arg("device", &device.name),
        );
    } else {
        anyhow::bail!("Live output switching requires native system audio");
    }
    *pin_output = selected.is_some();
    *output_index = selected
        .and_then(|device| outputs.iter().position(|item| item.id == device.id))
        .or_else(|| outputs.iter().position(|device| device.is_default));
    Ok(())
}

pub(super) fn apply_application_scope(
    store: &Store,
    runtime: &serde_json::Value,
    pids: &[i32],
    output: Option<&DeviceInfo>,
) -> Result<()> {
    if runtime["active"] == true && audio::native_controls(runtime) {
        return control::request_applications(store, pids);
    }
    ensure!(
        control::is_stopped(store),
        "Application capture requires native system audio"
    );
    if pids.is_empty() {
        return launch_system(store, output);
    }
    let mut args = vec!["application".to_owned()];
    for pid in pids {
        args.extend(["--pid".into(), pid.to_string()]);
    }
    if let Some(output) = output {
        args.extend(["--output".into(), output.id.clone()]);
    }
    args.push("--accept-routing".into());
    desktop::launch_audio(store, &args).map(|_| ())
}
