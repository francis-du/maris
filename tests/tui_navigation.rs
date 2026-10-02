use crossterm::event::KeyCode;
use maris::{
    audio::DeviceInfo,
    control_panel::{self, UndoTarget},
};
use serde_json::json;

fn device(id: &str, name: &str) -> DeviceInfo {
    DeviceInfo {
        id: id.into(),
        name: name.into(),
        direction: "output".into(),
        is_default: true,
    }
}

#[test]
fn actual_runtime_output_wins_over_default_and_same_name_on_all_native_backends() {
    for (backend, platform, prefix) in [
        ("coreaudio_process_tap", "macos", "uid:"),
        ("pulse_server", "linux", "pulse:"),
        ("wasapi_process_loopback", "windows", "wasapi:"),
    ] {
        let options = [
            device(&format!("{prefix}a"), "Headphones"),
            device(&format!("{prefix}b"), "Headphones"),
        ];
        let runtime = json!({"active":true, "system_backend":backend, "output_mode":"follow_system_default", "output":"Headphones", "device_identity":{"platform":platform,"stable_id":"b"}});
        assert_eq!(
            control_panel::runtime_output_index(&options, &runtime),
            Some(1)
        );
        assert_eq!(
            control_panel::runtime_output_index(&options[..1], &runtime),
            None
        );
        assert_eq!(
            control_panel::runtime_output_index(
                &[options[1].clone(), options[1].clone()],
                &runtime
            ),
            None
        );
        assert_eq!(
            control_panel::runtime_output_index(
                &options,
                &json!({"active":true,"output":"Headphones"})
            ),
            None
        );
        assert_eq!(
            control_panel::runtime_output_index(
                &options,
                &json!({"active":false,"output":"Headphones"})
            ),
            None
        );
    }
}

#[test]
fn duplicate_ids_and_input_endpoints_cannot_be_confirmed_as_outputs() {
    let mut input = device("uid:a", "A");
    input.direction = "input".into();
    assert!(control_panel::confirmed_output(&[input.clone()], 1, &[input]).is_err());
    let duplicate = [device("uid:a", "A"), device("uid:a", "A")];
    assert!(control_panel::confirmed_output(&duplicate[..1], 1, &duplicate).is_err());
    assert!(control_panel::selected_device_index(&duplicate, Some("uid:a")).is_err());
}

#[test]
fn tab_enters_and_cycles_only_the_five_optional_details() {
    use maris::control_panel::Workspace;
    let mut view = Workspace::Now.next();
    for detail in Workspace::DETAILS {
        assert_eq!(view, detail);
        assert_ne!(view, Workspace::Now);
        view = view.next();
    }
    assert_eq!(view, Workspace::Sound);
    assert_eq!(Workspace::Now.previous(), Workspace::System);
    assert_eq!(Workspace::Sound.previous(), Workspace::System);
}

#[test]
fn explicit_unknown_or_ambiguous_output_never_falls_back_to_default() {
    let options = [device("uid:a", "Headphones"), device("uid:b", "Headphones")];
    assert!(control_panel::selected_device_index(&options, Some("missing")).is_err());
    assert!(control_panel::selected_device_index(&options, Some("Headphones")).is_err());
    assert_eq!(
        control_panel::selected_device_index(&options, Some("uid:b")).unwrap(),
        Some(1)
    );
    assert_eq!(
        control_panel::selected_device_index(&options, None).unwrap(),
        None
    );
}

#[test]
fn vanished_uid_never_inherits_a_same_name_endpoints_identity() {
    let previous = device("uid:a", "Headphones");
    let replacement = [device("uid:b", "Headphones")];
    assert_eq!(
        control_panel::preserved_device_index(&replacement, Some(&previous)),
        None
    );
    let renamed = [device("uid:a", "Renamed headphones")];
    assert_eq!(
        control_panel::preserved_device_index(&renamed, Some(&previous)),
        Some(0)
    );
}

#[test]
fn output_confirmation_uses_frozen_picker_identity_not_refreshed_row_number() {
    let options = [device("uid:a", "A"), device("uid:b", "B")];
    let current = [options[1].clone(), options[0].clone()];
    assert_eq!(
        control_panel::confirmed_output(&options, 1, &current)
            .unwrap()
            .unwrap()
            .id,
        "uid:a"
    );
    assert!(control_panel::confirmed_output(&options, 1, &current[..1]).is_err());
    assert!(control_panel::confirmed_output(&options, 99, &current).is_err());
    assert!(control_panel::confirmed_output(&options, 0, &current)
        .unwrap()
        .is_none());
}

#[test]
fn compact_monitor_blocks_hidden_adjustments_but_allows_visible_dialogs() {
    for key in [
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char(' '),
        KeyCode::Char('d'),
        KeyCode::Char('a'),
    ] {
        assert!(!control_panel::compact_key_allowed(key));
    }
    for key in [
        KeyCode::Char('o'),
        KeyCode::Char('p'),
        KeyCode::Char('j'),
        KeyCode::Enter,
        KeyCode::Esc,
    ] {
        assert!(control_panel::compact_key_allowed(key));
    }
}

#[test]
fn app_renderer_and_selection_share_the_same_valid_pid_filter() {
    let apps = json!({"applications":[{"pid":0},{"pid":-1},{"pid":2147483648_i64},{"pid":7,"is_maris":true},{"pid":42,"bundle_id":"example"}]});
    let visible = control_panel::visible_applications(&apps);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0]["pid"], 42);
}

#[test]
fn undo_domain_covers_restored_controls_without_treating_eq_as_listening() {
    for row in [0, 1, 5, 9, 14, 15] {
        assert_eq!(control_panel::edit_target(row), UndoTarget::Listening);
    }
    for row in [10, 11, 12, 13, 16, 25] {
        assert_eq!(control_panel::edit_target(row), UndoTarget::Profile);
    }
}
