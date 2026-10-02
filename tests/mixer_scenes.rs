//! Named scene writes participate in the same revision checks as channel edits.
use maris::{control::store::Store, mixer};
use std::process::Command;

#[test]
fn scene_save_advances_revision_once_without_consuming_audio_undo() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let initial = mixer::edit(&store, Some(0), |config| {
        config.buses[0].gain_db = -3.0;
        Ok(())
    })
    .unwrap();
    let saved = mixer::save_scene_checked(&store, Some(initial.revision), "Quiet").unwrap();
    assert_eq!(saved.revision, initial.revision + 1);
    assert_eq!(saved.config, initial.config);
    assert_eq!(saved.previous, initial.previous);
    let bytes = std::fs::read(directory.path().join("mixer.json")).unwrap();
    let same = mixer::save_scene_checked(&store, Some(saved.revision), "Quiet").unwrap();
    assert_eq!(same.revision, saved.revision);
    assert_eq!(
        std::fs::read(directory.path().join("mixer.json")).unwrap(),
        bytes
    );
    assert!(mixer::save_scene_checked(&store, Some(initial.revision), "Quiet").is_err());
}

#[test]
fn stale_save_and_restore_leave_the_current_scene_and_settings_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let saved = mixer::save_scene_checked(&store, Some(0), "Default").unwrap();
    let current = mixer::edit(&store, Some(saved.revision), |config| {
        config.buses[1].gain_db = -4.0;
        Ok(())
    })
    .unwrap();
    let bytes = std::fs::read(directory.path().join("mixer.json")).unwrap();
    assert!(mixer::save_scene_checked(&store, Some(saved.revision), "Default").is_err());
    assert!(mixer::restore_scene_checked(&store, Some(saved.revision), "Default").is_err());
    assert_eq!(
        std::fs::read(directory.path().join("mixer.json")).unwrap(),
        bytes
    );
    let restored = mixer::restore_scene_checked(&store, Some(current.revision), "Default").unwrap();
    assert_eq!(restored.config, saved.config);
    assert_eq!(restored.previous, Some(current.config));
}

#[test]
fn concurrent_scene_writers_cannot_both_accept_the_same_revision() {
    let directory = tempfile::tempdir().unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for name in ["One", "Two"] {
        let path = directory.path().to_path_buf();
        let gate = gate.clone();
        workers.push(std::thread::spawn(move || {
            gate.wait();
            mixer::save_scene_checked(&Store::at(path), Some(0), name).is_ok()
        }));
    }
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| usize::from(worker.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let state = mixer::load(&Store::at(directory.path())).unwrap();
    assert_eq!(state.revision, 1);
    assert_eq!(state.scenes.len(), 1);
}

#[test]
fn cli_scene_actions_obey_expected_revision_instead_of_ignoring_it() {
    let directory = tempfile::tempdir().unwrap();
    let run = |revision: &str, action: &str| {
        Command::new(env!("CARGO_BIN_EXE_maris"))
            .env("MARIS_STATE_DIR", directory.path())
            .args([
                "--json",
                "--expected-revision",
                revision,
                "mixer",
                action,
                "Saved",
            ])
            .output()
            .unwrap()
    };
    assert!(run("0", "scene-save").status.success());
    let before = std::fs::read(directory.path().join("mixer.json")).unwrap();
    for action in ["scene-save", "scene-restore"] {
        let failed = run("0", action);
        assert!(!failed.status.success(), "{action}");
        let value: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(
            std::fs::read(directory.path().join("mixer.json")).unwrap(),
            before
        );
    }
    assert!(run("1", "scene-restore").status.success());
}
