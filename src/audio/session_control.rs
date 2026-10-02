//! Session-bound control commands consumed by the audio control thread, never by callbacks.

use super::Session;
use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Poll saved configuration on the control thread. A full queue must not
/// acknowledge a revision that the renderer never received; the next tick retries it.
pub(super) fn queue_settings(
    store: &crate::control::store::Store,
    output_key: &str,
    rate: u32,
    updates: &crossbeam_queue::ArrayQueue<super::Update>,
    sent_revision: &mut u64,
    sent_music_revision: &mut Option<u64>,
    sent_capability: &mut crate::devices::capability::Capability,
) -> Result<()> {
    let state = store.load()?;
    let listening = crate::tuning::preferences::load(store)?;
    let capability = crate::devices::capability::effective(store, output_key)?;
    // Device limits have their own storage. A changed limit must reach the
    // renderer even when neither EQ nor listening preferences were edited.
    if state.revision != *sent_revision
        || *sent_music_revision != Some(listening.revision)
        || capability != *sent_capability
    {
        let effective_music = crate::devices::capability::apply_constraints(
            listening.effective(output_key),
            &capability,
        );
        let update = super::Update {
            settings: crate::dsp::Settings::compile(&state.profile, rate)?
                .with_music(&effective_music, rate)?,
            revision: state.revision,
            music_revision: listening.revision,
        };
        if updates.push(update).is_ok() {
            *sent_revision = state.revision;
            *sent_music_revision = Some(listening.revision);
            *sent_capability = capability;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn parse_pids(command: &Value) -> Result<Vec<i32>> {
    let values = command["pids"]
        .as_array()
        .context("Missing application PIDs")?;
    anyhow::ensure!(values.len() <= 64, "Too many application PIDs");
    values
        .iter()
        .map(|value| {
            value
                .as_i64()
                .and_then(|pid| i32::try_from(pid).ok())
                .filter(|pid| *pid > 0)
                .context("Invalid application PID")
        })
        .collect()
}

impl Session {
    pub(super) fn handle_control_command(&mut self) -> Result<()> {
        let command = match crate::control::take_command(&self.store, &self.session_id) {
            Ok(Some(command)) => command,
            Ok(None) => return Ok(()),
            Err(error) => {
                self.record_control_result("invalid_control", Some(error.to_string()));
                return Ok(());
            }
        };
        let action = match crate::control::validated_action(&command, self.rebind_count) {
            Ok(action) => action.to_owned(),
            Err(error) => {
                self.record_control_result("invalid_control", Some(error.to_string()));
                return Ok(());
            }
        };

        match action.as_str() {
            "stop" => {
                self.stop_requested = true;
                self.record_control_result(&action, None);
                Ok(())
            }
            "select_output" => self.handle_output_control(&action, &command),
            "select_applications" => self.handle_application_control(&action, &command),
            _ => {
                self.record_control_result(
                    &action,
                    Some(format!("Unsupported control action: {action}")),
                );
                Ok(())
            }
        }
    }

    pub(super) fn record_control_result(&mut self, action: &str, error: Option<String>) {
        let result = json!({
            "session_id": self.session_id,
            "action": action,
            "ok": error.is_none(),
            "error": error,
            "updated_at_ms": crate::analysis::now_ms()
        });
        self.last_control_result = Some(result.clone());
        let _ = self.store.write_json("control-result.json", &result);
    }

    fn finish_control_result(&mut self, action: &str, result: Result<()>) -> Result<()> {
        match result {
            Ok(()) => {
                self.record_control_result(action, None);
                Ok(())
            }
            Err(error) => {
                let message = format!("{error:#}");
                self.record_control_result(action, Some(message));
                if self.route_healthy_after_control_error() {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn handle_output_control(&mut self, action: &str, command: &Value) -> Result<()> {
        if self.tap.is_none() {
            self.record_control_result(
                action,
                Some("Output switching requires native system audio".into()),
            );
            return Ok(());
        }
        let requested = match crate::control::parse_output_selection(command) {
            Ok(output) => output,
            Err(error) => {
                self.record_control_result(action, Some(error.to_string()));
                return Ok(());
            }
        };
        let result = self.rebind_system_output(requested);
        self.finish_control_result(action, result)
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_output_control(&mut self, action: &str, command: &Value) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.pulse.is_some() {
            let result = crate::control::parse_output_selection(command)
                .and_then(|output| self.rebind_pulse(output));
            return self.finish_control_result(action, result);
        }
        #[cfg(target_os = "windows")]
        if self.windows.is_some() {
            let result = crate::control::parse_output_selection(command)
                .and_then(|output| self.rebind_windows_output(output));
            return self.finish_control_result(action, result);
        }
        let _ = command;
        self.record_control_result(
            action,
            Some("This session has no native system-audio route to switch".into()),
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn handle_application_control(&mut self, action: &str, command: &Value) -> Result<()> {
        if self.tap.is_none() {
            self.record_control_result(
                action,
                Some("Application capture switching requires native system audio".into()),
            );
            return Ok(());
        }
        let parsed = (|| -> Result<Vec<i32>> {
            let values = command["pids"]
                .as_array()
                .context("Application capture command is missing its PID list")?;
            anyhow::ensure!(values.len() <= 64, "Too many application PIDs");
            values
                .iter()
                .map(|value| {
                    value
                        .as_i64()
                        .and_then(|value| i32::try_from(value).ok())
                        .context("Invalid application PID")
                })
                .collect()
        })();
        let pids = match parsed {
            Ok(pids) => pids,
            Err(error) => {
                self.record_control_result(action, Some(format!("{error:#}")));
                return Ok(());
            }
        };
        let result = self.rebind_system_processes(&pids);
        self.finish_control_result(action, result)
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_application_control(&mut self, action: &str, command: &Value) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.pulse.is_some() {
            let result = parse_pids(command).and_then(|pids| self.rebind_pulse_processes(&pids));
            return self.finish_control_result(action, result);
        }
        #[cfg(target_os = "windows")]
        if self.windows.is_some() {
            let result = parse_pids(command).and_then(|pids| self.rebind_windows_processes(&pids));
            return self.finish_control_result(action, result);
        }
        let _ = command;
        self.record_control_result(
            action,
            Some("This session has no native application-audio route to switch".into()),
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn route_healthy_after_control_error(&self) -> bool {
        self.tap.is_some() && !self.streams.is_empty()
    }

    #[cfg(not(target_os = "macos"))]
    fn route_healthy_after_control_error(&self) -> bool {
        #[cfg(target_os = "linux")]
        if self.pulse.is_some() {
            return self
                .metrics
                .errors
                .load(std::sync::atomic::Ordering::Relaxed)
                == 0
                && self.pulse.as_ref().is_some_and(|p| p.has_worker());
        }
        #[cfg(target_os = "windows")]
        if self.windows.is_some() {
            return self
                .windows
                .as_ref()
                .is_some_and(super::windows::Native::healthy)
                && !self.streams.is_empty();
        }
        !self.streams.is_empty()
    }
}
