//! The real command consumer uses temporary files, never a live audio session.
use super::*;

fn fixture() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    store
        .write_json(
            "runtime.json",
            &json!({"active":true,"session_id":"queue-test",
        "system_backend":"coreaudio_process_tap","updated_at_ms":crate::analysis::now_ms()}),
        )
        .unwrap();
    (directory, store)
}

#[test]
fn malformed_command_reports_failure_and_does_not_block_later_controls() {
    for invalid in [
        b"{broken".to_vec(),
        vec![b'x'; (store::MAX_JSON_BYTES + 1) as usize],
        b"null".to_vec(),
        b"[]".to_vec(),
        b"{\"action\":\"stop\"}".to_vec(),
        b"{\"session_id\":42,\"action\":\"stop\"}".to_vec(),
    ] {
        let (_directory, store) = fixture();
        let runtime = std::fs::read(store.directory.join("runtime.json")).unwrap();
        std::fs::write(store.directory.join("control.json"), invalid).unwrap();
        assert!(
            take_command(&store, "queue-test").is_err(),
            "invalid command was silently ignored"
        );
        assert!(
            !store.directory.join("control.json").exists(),
            "invalid command still blocks future input"
        );
        assert_eq!(
            std::fs::read(store.directory.join("runtime.json")).unwrap(),
            runtime
        );
        request_output(&store, Some("Headphones")).unwrap();
        let command = take_command(&store, "queue-test").unwrap().unwrap();
        assert_eq!(command["output"], "Headphones");
        assert!(take_command(&store, "queue-test").unwrap().is_none());
    }
}

#[test]
fn other_sessions_commands_are_never_consumed_or_retargeted() {
    let (_directory, store) = fixture();
    let command = json!({"session_id":"another-session","action":"stop"});
    store.write_json("control.json", &command).unwrap();
    let bytes = std::fs::read(store.directory.join("control.json")).unwrap();
    assert!(take_command(&store, "queue-test").unwrap().is_none());
    assert_eq!(
        std::fs::read(store.directory.join("control.json")).unwrap(),
        bytes
    );
    assert_eq!(
        take_command(&store, "another-session").unwrap(),
        Some(command)
    );
}

#[test]
fn directories_and_links_are_not_opened_as_audio_commands() {
    let (_directory, store) = fixture();
    std::fs::create_dir(store.directory.join("control.json")).unwrap();
    assert!(take_command(&store, "queue-test").is_err());
    assert!(store.directory.join("control.json").is_dir());
}

#[cfg(unix)]
#[test]
fn linked_command_cannot_consume_a_different_file() {
    let (_directory, store) = fixture();
    let target = store.directory.join("unrelated.json");
    std::fs::write(
        &target,
        b"{\"session_id\":\"queue-test\",\"action\":\"stop\"}",
    )
    .unwrap();
    let bytes = std::fs::read(&target).unwrap();
    std::os::unix::fs::symlink(&target, store.directory.join("control.json")).unwrap();
    assert!(take_command(&store, "queue-test").is_err());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert!(
        std::fs::symlink_metadata(store.directory.join("control.json"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
