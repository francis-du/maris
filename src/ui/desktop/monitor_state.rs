//! Shared, testable status text for the native listening monitor.
use crate::i18n::text as t;
use serde_json::Value;

pub fn expired_session(runtime: &Value) -> bool {
    runtime["stale"] == true
        && runtime["session_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
}

pub fn menu_mode(
    runtime: &Value,
    startup: &Value,
    application: &'static str,
    stopped: bool,
) -> &'static str {
    if runtime["active"] != true {
        if startup["phase"] == "failed" {
            "Audio unavailable"
        } else if startup["phase"] == "requesting_system_audio" && !stopped {
            "Awaiting authorization"
        } else if expired_session(runtime) {
            "Awaiting telemetry"
        } else {
            "Standby"
        }
    } else if application != "Applied" {
        application
    } else if runtime["tonal_bypass"] == true {
        "Processing bypassed"
    } else if runtime["neural"].is_object() {
        "Voice AI"
    } else {
        "Processing"
    }
}

pub fn status(runtime: &Value, bypass: bool, now: u64) -> String {
    let live = crate::ui::tui::studio::live(runtime, now);
    let mode = t(
        if expired_session(runtime) || (runtime["active"] == true && !live) {
            "STALE"
        } else if !live {
            "Standby"
        } else if bypass {
            "EQ bypassed"
        } else {
            "Processing"
        },
    );
    if !live {
        return mode.to_owned();
    }
    let level = runtime["peak_dbfs"]
        .as_f64()
        .filter(|value| value.is_finite())
        .map_or_else(
            || t("Unavailable").to_owned(),
            |value| format!("{value:.1} dBFS"),
        );
    let adaptive = runtime["adaptive_reduction_db"]
        .as_f64()
        .filter(|value| !bypass && value.is_finite() && *value > 0.05);
    match adaptive {
        Some(value) => format!("{mode} · {level} · {} -{value:.1} dB", t("Dynamic EQ")),
        None => format!("{mode} · {level}"),
    }
}
