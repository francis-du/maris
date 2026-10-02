use maris::platform;

#[test]
fn platform_capabilities_match_the_wired_native_paths() {
    let capability = platform::capabilities();
    assert!(!capability.changes_system_default_output);
    assert!(!capability.changes_system_volume);
    assert!(capability.real_device_validation_required);

    if cfg!(target_os = "macos") {
        assert!(capability.automatic_system_takeover);
        assert_eq!(
            capability.active_backend.as_deref(),
            Some("coreaudio_process_tap")
        );
        assert!(capability.stable_device_identity);
        assert!(capability.application_metadata);
        assert!(capability.per_application_capture);
        assert!(capability.per_application_routing);
        assert!(capability.per_application_volume);
        assert!(capability.simultaneous_hardware_inputs);
        assert!(capability.simultaneous_hardware_outputs);
        assert!(!capability.changes_application_session_mute);
        assert!(capability.candidate_backend.is_none());
    } else if cfg!(target_os = "linux") {
        assert!(capability.automatic_system_takeover);
        assert_eq!(capability.active_backend.as_deref(), Some("pulse_server"));
        assert!(capability.per_application_capture);
        assert!(capability.per_application_routing);
        assert!(capability.per_application_volume);
        assert!(capability.simultaneous_hardware_inputs);
        assert!(capability.simultaneous_hardware_outputs);
        assert!(capability.live_output_pin);
        assert!(!capability.changes_application_session_mute);
        assert!(capability.candidate_backend.is_none());
    } else if cfg!(target_os = "windows") {
        assert!(capability.automatic_system_takeover);
        assert_eq!(
            capability.active_backend.as_deref(),
            Some("wasapi_process_loopback")
        );
        assert!(capability.application_metadata);
        assert!(capability.per_application_capture);
        assert!(capability.per_application_routing);
        assert!(capability.per_application_volume);
        assert!(capability.simultaneous_hardware_inputs);
        assert!(capability.simultaneous_hardware_outputs);
        assert!(capability.live_output_pin);
        assert!(capability.changes_application_session_mute);
        assert!(capability.candidate_backend.is_none());
    } else {
        assert!(!capability.automatic_system_takeover);
        assert!(capability.active_backend.is_none());
        assert!(!capability.per_application_capture);
        assert!(!capability.per_application_routing);
        assert!(!capability.per_application_volume);
        assert!(!capability.simultaneous_hardware_inputs);
        assert!(!capability.simultaneous_hardware_outputs);
    }
}
