//! Per-session cooperative commands. No PID signalling or arbitrary process execution.
pub mod mcp;
pub mod store;

use crate::{audio, control::store::Store};
use anyhow::{Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};

/// Serialize command producers with the control-thread consumer. Stop has priority;
/// unrelated pending route changes may not be silently overwritten by another UI.
pub(crate) fn command_lock(store: &Store) -> Result<std::fs::File> {
    std::fs::create_dir_all(&store.directory)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(store.directory.join("control.lock"))?;
    file.lock_exclusive()?;
    Ok(file)
}

/// Read and consume a command under the same lock used by every producer.
pub(crate) fn take_command(store: &Store, session: &str) -> Result<Option<Value>> {
    use std::io::Read;
    let path = store.directory.join("control.json");
    // Avoid creating a lock file on every idle control tick.
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let _lock = command_lock(store)?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "Audio command must be a regular file, not a directory or link"
    );
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(&path)?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "Audio command must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(store::MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let command = if bytes.len() as u64 <= store::MAX_JSON_BYTES {
        serde_json::from_slice::<Value>(&bytes).ok()
    } else {
        None
    };
    let valid_session = command
        .as_ref()
        .and_then(|command| command["session_id"].as_str())
        .is_some_and(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control));
    if !valid_session {
        // Discard only invalid content, under the producer lock. Actual read or
        // permission errors above leave the file intact for diagnosis.
        std::fs::remove_file(&path).context("Remove invalid audio command")?;
        anyhow::bail!("Invalid audio command was discarded; submit the change again");
    }
    let command = command.expect("validated command session");
    if command["session_id"].as_str() != Some(session) {
        return Ok(None);
    }
    std::fs::remove_file(path)?;
    Ok(Some(command))
}

fn queue(store: &Store, state: &Value, mut command: Value, stop: bool) -> Result<()> {
    let _lock = command_lock(store)?;
    let current = audio::runtime_status(store);
    anyhow::ensure!(
        current["active"] == true && current["session_id"] == state["session_id"],
        "Audio session changed; select again"
    );
    if !stop {
        anyhow::ensure!(
            current["rebind_count"] == state["rebind_count"]
                && current["output"] == state["output"],
            "Active output changed after preview"
        );
        if let Some(count) = state["rebind_count"].as_u64() {
            command["expected_rebind_count"] = json!(count);
        }
    }
    let path = store.directory.join("control.json");
    if path.exists() && !stop {
        let pending: Value = crate::control::store::read_json(&path)?;
        if pending["session_id"] == command["session_id"] {
            if pending == command {
                return Ok(());
            }
            anyhow::bail!("An audio change is already pending");
        }
    }
    store.write_json("control.json", &command)
}

pub fn request_stop(store: &Store) -> Result<()> {
    let state = audio::runtime_status(store);
    if state["active"].as_bool() != Some(true) {
        return Ok(());
    }
    let session_id = state["session_id"]
        .as_str()
        .context("The active session does not support cooperative stop")?;
    queue(
        store,
        &state,
        json!({"session_id":session_id,"action":"stop"}),
        true,
    )
}
/// Pure validation used by the control-thread consumer. Malformed commands produce
/// a failed result, not an engine shutdown or an accidental default route.
pub fn validated_action(command: &Value, rebind_count: u64) -> Result<&str> {
    let action = command["action"]
        .as_str()
        .context("Control command is missing its action")?;
    anyhow::ensure!(
        matches!(action, "stop" | "select_output" | "select_applications"),
        "Unsupported control action"
    );
    if action != "stop" {
        if let Some(expected) = command.get("expected_rebind_count") {
            let count = expected.as_u64().context("Invalid output revision guard")?;
            anyhow::ensure!(count == rebind_count, "Active output changed after preview");
        }
    }
    if action == "select_output" {
        parse_output_selection(command)?;
    }
    Ok(action)
}

/// Missing/wrongly typed fields must never mean "follow system default".
pub fn parse_output_selection(command: &Value) -> Result<Option<&str>> {
    match command.get("output") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if !value.is_empty() && value.len() <= 516 && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value))
        }
        _ => anyhow::bail!("Output must be a selector or explicit null"),
    }
}

pub fn request_output(store: &Store, output: Option<&str>) -> Result<()> {
    request_output_checked(store, None, output)
}

/// A confirmed menu selection may not retarget itself to a newly started audio session.
pub fn request_output_for_session(
    store: &Store,
    session: &str,
    output: Option<&str>,
) -> Result<()> {
    request_output_checked(store, Some(session), output)
}

fn request_output_checked(
    store: &Store,
    expected_session: Option<&str>,
    output: Option<&str>,
) -> Result<()> {
    if let Some(output) = output {
        anyhow::ensure!(
            !output.is_empty() && output.len() <= 516 && !output.chars().any(char::is_control),
            "Invalid output selector"
        );
    }
    let state = audio::runtime_status(store);
    anyhow::ensure!(state["active"] == true, "No active audio session");
    anyhow::ensure!(
        audio::native_controls(&state),
        "Live output switching is available only for native system audio"
    );
    let session_id = state["session_id"]
        .as_str()
        .context("The active session does not support output switching")?;
    if let Some(expected) = expected_session {
        anyhow::ensure!(
            session_id == expected
                && crate::ui::tui::studio::live(&state, crate::analysis::now_ms()),
            "Audio session changed; select again"
        );
    }
    queue(
        store,
        &state,
        json!({"session_id":session_id,"action":"select_output","output":output}),
        false,
    )
}

pub fn request_applications(store: &Store, pids: &[i32]) -> Result<()> {
    let state = audio::runtime_status(store);
    anyhow::ensure!(state["active"] == true, "No active audio session");
    anyhow::ensure!(
        audio::native_controls(&state),
        "Application capture switching requires a supported native audio session"
    );
    anyhow::ensure!(
        pids.len() <= 64,
        "At most 64 application PIDs can be selected"
    );
    let session_pid = state["pid"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok());
    for (index, pid) in pids.iter().copied().enumerate() {
        anyhow::ensure!(
            pid > 0 && session_pid != Some(pid),
            "Invalid application PID {pid}"
        );
        anyhow::ensure!(
            !pids[..index].contains(&pid),
            "Duplicate application PID {pid}"
        );
    }
    let session_id = state["session_id"]
        .as_str()
        .context("The active session does not support application capture switching")?;
    queue(
        store,
        &state,
        json!({"session_id":session_id,"action":"select_applications","pids":pids}),
        false,
    )
}

pub fn is_stopped(store: &Store) -> bool {
    store.session_lock().is_ok()
}

#[cfg(test)]
#[path = "../../tests/unit/command_queue.rs"]
mod command_queue_tests;
