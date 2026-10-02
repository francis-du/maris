use maris::control;
use serde_json::json;

#[test]
fn malformed_actions_and_revision_guards_fail_without_becoming_a_route() {
    for command in [
        json!({"session_id":"session"}),
        json!({"action":false}),
        json!({"action":"unknown"}),
        json!({"action":"select_output"}),
        json!({"action":"select_output","output":false}),
        json!({"action":"select_output","output":null,"expected_rebind_count":null}),
        json!({"action":"select_output","output":null,"expected_rebind_count":-1}),
        json!({"action":"select_output","output":null,"expected_rebind_count":"0"}),
        json!({"action":"select_output","output":null,"expected_rebind_count":1}),
    ] {
        assert!(
            control::validated_action(&command, 0).is_err(),
            "accepted {command}"
        );
    }
}

#[test]
fn explicit_legacy_output_commands_still_validate_without_a_revision_guard() {
    assert_eq!(
        control::validated_action(&json!({"action":"select_output","output":null}), 0).unwrap(),
        "select_output"
    );
    assert_eq!(
        control::validated_action(
            &json!({"action":"select_output","output":"uid:headphones","expected_rebind_count":2}),
            2
        )
        .unwrap(),
        "select_output"
    );
}

#[test]
fn emergency_stop_remains_valid_across_output_rebinds() {
    assert_eq!(
        control::validated_action(&json!({"action":"stop","expected_rebind_count":0}), 100)
            .unwrap(),
        "stop"
    );
}
