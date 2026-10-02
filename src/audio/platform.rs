//! Runtime platform-audio capabilities. Only wired product paths are marked implemented.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SystemAudioCapabilities {
    pub platform: String,
    pub automatic_system_takeover: bool,
    pub active_backend: Option<String>,
    pub candidate_backend: Option<String>,
    pub automatic_output_follow: bool,
    pub live_output_pin: bool,
    pub stable_device_identity: bool,
    pub application_metadata: bool,
    pub per_application_capture: bool,
    pub per_application_routing: bool,
    pub per_application_volume: bool,
    pub simultaneous_hardware_inputs: bool,
    pub simultaneous_hardware_outputs: bool,
    pub changes_system_default_output: bool,
    pub changes_system_volume: bool,
    pub changes_application_session_mute: bool,
    pub driver_required_for_normal_path: bool,
    pub real_device_validation_required: bool,
    pub notes: Vec<String>,
}

pub fn capabilities() -> SystemAudioCapabilities {
    match std::env::consts::OS {
        "macos" => SystemAudioCapabilities {
            platform: "macos".into(),
            automatic_system_takeover: true,
            active_backend: Some("coreaudio_process_tap".into()),
            candidate_backend: None,
            automatic_output_follow: true,
            live_output_pin: true,
            stable_device_identity: true,
            application_metadata: true,
            per_application_capture: true,
            per_application_routing: true,
            per_application_volume: true,
            simultaneous_hardware_inputs: true,
            simultaneous_hardware_outputs: true,
            changes_system_default_output: false,
            changes_system_volume: false,
            changes_application_session_mute: false,
            driver_required_for_normal_path: false,
            real_device_validation_required: true,
            notes: vec![
                "Native system playback uses a private CoreAudio process tap and excludes Maris."
                    .into(),
                "Selected application PIDs can use the shared listening path or independent mixer strips with separate gain, EQ and compression."
                    .into(),
                "CoreAudio device UID/model UID provide stable device evidence when available."
                    .into(),
            ],
        },
        "windows" => SystemAudioCapabilities {
            platform: "windows".into(),
            automatic_system_takeover: true,
            active_backend: Some("wasapi_process_loopback".into()),
            candidate_backend: None,
            automatic_output_follow: false,
            live_output_pin: true,
            stable_device_identity: false,
            application_metadata: true,
            per_application_capture: true,
            per_application_routing: true,
            per_application_volume: true,
            simultaneous_hardware_inputs: true,
            simultaneous_hardware_outputs: true,
            changes_system_default_output: false,
            changes_system_volume: false,
            changes_application_session_mute: true,
            driver_required_for_normal_path: false,
            real_device_validation_required: true,
            notes: vec![
                "WASAPI system/process loopback feeds the existing Maris DSP and physical output path on supported Windows builds."
                    .into(),
                "Explicit system/application sessions temporarily mute only the original render sessions that Maris replaces, record each prior mute state, and restore it on exit. Session volume scalars and the Windows default output are not changed."
                    .into(),
                "Independent application mixer strips use one process-loopback source per PID."
                    .into(),
            ],
        },
        "linux" => SystemAudioCapabilities {
            platform: "linux".into(),
            automatic_system_takeover: true,
            active_backend: Some("pulse_server".into()),
            candidate_backend: None,
            automatic_output_follow: true,
            live_output_pin: true,
            stable_device_identity: false,
            application_metadata: true,
            per_application_capture: true,
            per_application_routing: true,
            per_application_volume: true,
            simultaneous_hardware_inputs: true,
            simultaneous_hardware_outputs: true,
            changes_system_default_output: false,
            changes_system_volume: false,
            changes_application_session_mute: false,
            driver_required_for_normal_path: false,
            real_device_validation_required: true,
            notes: vec![
                "The local PulseAudio-compatible backend covers native PulseAudio and PipeWire's Pulse service without installing a driver."
                    .into(),
                "An owned temporary sink, PCM worker, route journal and parent-exit watcher process and restore selected streams without changing the system default output or volume."
                    .into(),
                "Application mixer strips use independent routed capture streams; native-server and real-device validation remain separate release evidence."
                    .into(),
            ],
        },
        other => SystemAudioCapabilities {
            platform: other.into(),
            automatic_system_takeover: false,
            active_backend: None,
            candidate_backend: None,
            automatic_output_follow: false,
            live_output_pin: false,
            stable_device_identity: false,
            application_metadata: false,
            per_application_capture: false,
            per_application_routing: false,
            per_application_volume: false,
            simultaneous_hardware_inputs: false,
            simultaneous_hardware_outputs: false,
            changes_system_default_output: false,
            changes_system_volume: false,
            changes_application_session_mute: false,
            driver_required_for_normal_path: false,
            real_device_validation_required: true,
            notes: vec!["This operating system is not a supported Maris target.".into()],
        },
    }
}
