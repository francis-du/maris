//! Shared, testable status text for the native listening monitor.
use crate::i18n::text as t;
use serde_json::Value;

pub fn status(runtime: &Value, bypass: bool, now: u64) -> String {
    let live = crate::ui::tui::studio::live(runtime, now);
    let mode = t(if runtime["active"] == true && !live {
        "STALE"
    } else if !live {
        "Standby"
    } else if bypass {
        "EQ bypassed"
    } else {
        "Processing"
    });
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
