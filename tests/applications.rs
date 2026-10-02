use maris::{audio, platform};

#[test]
fn application_metadata_is_read_only_and_matches_platform_capabilities() {
    let state = audio::applications().unwrap();
    let capability = platform::capabilities();
    assert!(state["available"].is_boolean());
    assert!(state["per_application_volume"].is_boolean());
    assert!(state["applications"].as_array().is_some());
    if state["available"] == true {
        assert_eq!(
            state["per_application_capture"].as_bool(),
            Some(capability.per_application_capture)
        );
        assert_eq!(
            state["per_application_routing"].as_bool(),
            Some(capability.per_application_routing)
        );
        assert_eq!(
            state["per_application_volume"].as_bool(),
            Some(capability.per_application_volume)
        );
        assert!(state["backend"].as_str().is_some());
        for application in state["applications"].as_array().unwrap() {
            assert!(application["object_id"].is_u64());
            assert!(application["pid"].is_i64() || application["pid"].is_u64());
            assert!(application["running_output"].is_boolean());
            assert!(application["devices"].as_array().is_some());
        }
        #[cfg(target_os = "windows")]
        {
            let mut pids = std::collections::BTreeSet::new();
            for application in state["applications"].as_array().unwrap() {
                let pid = application["pid"].as_u64().unwrap();
                assert!((1..=i32::MAX as u64).contains(&pid));
                assert_ne!(pid, u64::from(std::process::id()));
                assert!(pids.insert(pid), "duplicate Windows application PID");
                assert!(application["name"].as_str().is_some());
                assert!(application["executable"].is_null());
            }
        }
    }
}
