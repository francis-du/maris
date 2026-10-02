//! Exercise the command path used by Mixer::tick without opening audio devices.
use super::*;

#[test]
fn another_sessions_stop_is_left_intact() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let value = json!({"session_id":"new-session","action":"stop"});
    store.write_json("control.json", &value).unwrap();
    let before = std::fs::read(dir.path().join("control.json")).unwrap();
    assert!(!take_mixer_stop(&store, "old-mixer").unwrap());
    assert_eq!(
        std::fs::read(dir.path().join("control.json")).unwrap(),
        before
    );
    assert!(take_mixer_stop(&store, "new-session").unwrap());
    assert!(!take_mixer_stop(&store, "new-session").unwrap());
}

#[test]
fn idle_mixer_does_not_create_a_control_lock() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    assert!(!take_mixer_stop(&store, "mixer").unwrap());
    assert!(!dir.path().join("control.lock").exists());
}

#[test]
fn invalid_command_is_reported_without_blocking_a_later_stop() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    std::fs::write(dir.path().join("control.json"), b"{invalid").unwrap();
    assert!(take_mixer_stop(&store, "mixer").is_err());
    store
        .write_json(
            "control.json",
            &json!({"session_id":"mixer","action":"stop"}),
        )
        .unwrap();
    assert!(take_mixer_stop(&store, "mixer").unwrap());
}

#[cfg(unix)]
#[test]
fn linked_command_cannot_stop_the_mixer_or_remove_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let target = dir.path().join("unrelated.json");
    std::fs::write(&target, b"{\"session_id\":\"mixer\",\"action\":\"stop\"}").unwrap();
    let before = std::fs::read(&target).unwrap();
    let command = dir.path().join("control.json");
    std::os::unix::fs::symlink(&target, &command).unwrap();
    assert!(take_mixer_stop(&store, "mixer").is_err());
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert!(std::fs::symlink_metadata(&command)
        .unwrap()
        .file_type()
        .is_symlink());
}
