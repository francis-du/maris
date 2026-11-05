//! Per-application enumeration: which processes can be captured, and through
//! which platform backend.

use anyhow::Result;
use serde::Serialize;
#[cfg(not(target_os = "windows"))]
use serde_json::json;
use serde_json::Value;

#[cfg(target_os = "linux")]
use super::pulse;
#[cfg(target_os = "macos")]
use super::tap_ffi;
#[cfg(target_os = "windows")]
use super::windows;

#[derive(Clone, Debug, Serialize)]
pub struct ApplicationInfo {
    pub object_id: u32,
    pub pid: i32,
    pub bundle_id: Option<String>,
    pub running_output: bool,
    pub devices: Vec<String>,
    pub is_maris: bool,
}

pub fn applications() -> Result<Value> {
    #[cfg(target_os = "macos")]
    {
        Ok(match tap_ffi::applications() {
            Ok(applications) => json!({
                "available":true,
                "backend":"coreaudio_process_objects",
                "applications":applications,
                "per_application_capture_backend":"coreaudio_process_tap",
                "per_application_capture":true,
                "per_application_routing":true,
                "per_application_volume":true,
                "note":"Selected application PIDs can use the shared listening path or independent mixer strips with separate gain, EQ and compression."
            }),
            Err(error) => json!({
                "available":false,
                "backend":"coreaudio_process_objects",
                "applications":[],
                "per_application_capture_backend":"coreaudio_process_tap",
                "per_application_capture":false,
                "per_application_routing":false,
                "per_application_volume":false,
                "reason":format!("{error:#}"),
                "note":"CoreAudio process metadata was unavailable; no application state is invented."
            }),
        })
    }
    #[cfg(target_os = "linux")]
    {
        Ok(pulse::applications().unwrap_or_else(|error| json!({"available":false,
            "backend":"pulse_server","applications":[],"per_application_capture":false,
            "per_application_routing":false,"per_application_volume":false,"reason":format!("{error:#}")})))
    }
    #[cfg(target_os = "windows")]
    {
        windows::applications()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Ok(json!({
            "available":false,
            "backend":Value::Null,
            "applications":[],
            "per_application_capture":false,
            "per_application_routing":false,
            "per_application_volume":false,
            "note":"This operating system is outside Maris' supported macOS, Linux and Windows targets."
        }))
    }
}
