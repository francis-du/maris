use maris::{
    profile::Profile,
    store::{read_json, Store},
};
use serde_json::json;

#[test]
fn invalid_numbers_are_rejected() {
    for gain in [f64::NAN, f64::INFINITY, -25.0, 25.0] {
        let mut p = Profile::default();
        p.bands[0].gain_db = gain;
        assert!(p.validate().is_err());
    }
    let mut p = Profile::default();
    p.bands[0].q = 0.0;
    assert!(p.validate().is_err());
}
#[test]
fn unknown_profile_fields_are_rejected() {
    let mut value = serde_json::to_value(Profile::default()).unwrap();
    value["command"] = json!("not allowed");
    assert!(serde_json::from_value::<Profile>(value).is_err());
}
#[test]
fn legacy_builtin_minus_six_db_preamp_is_compatibly_normalized() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut legacy = Profile::preset("vocal").unwrap();
    legacy.preamp_db = -6.0;
    std::fs::create_dir_all(directory.path()).unwrap();
    std::fs::write(
        directory.path().join("profile.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "revision": 12,
            "profile": legacy,
            "previous": null
        }))
        .unwrap(),
    )
    .unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.revision, 12);
    assert_eq!(loaded.profile.name, "vocal");
    assert_eq!(loaded.profile.preamp_db, 0.0);
}

#[test]
fn custom_minus_six_db_preamp_is_not_silently_rewritten() {
    let mut profile = Profile::preset("vocal").unwrap();
    profile.name = "custom".into();
    profile.preamp_db = -6.0;
    profile.normalize_legacy_builtin_preamp();
    assert_eq!(profile.preamp_db, -6.0);
}

#[test]
fn revision_conflicts_do_not_overwrite_state() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let first = store
        .edit(Some(0), |p| {
            *p = Profile::preset("warm")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(first.revision, 1);
    assert!(store
        .edit(Some(0), |p| {
            p.preamp_db = -20.0;
            Ok(())
        })
        .is_err());
    assert_eq!(store.load().unwrap().profile, first.profile);
}
#[test]
fn invalid_edits_are_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let before = store.load().unwrap();
    assert!(store
        .edit(None, |p| {
            p.crossfeed = 2.0;
            Ok(())
        })
        .is_err());
    assert_eq!(store.load().unwrap().profile, before.profile);
    assert_eq!(store.load().unwrap().revision, before.revision);
}
#[test]
fn undo_swaps_two_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    store
        .edit(None, |p| {
            *p = Profile::preset("warm")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(store.undo(Some(1)).unwrap().profile.name, "flat");
    assert_eq!(store.undo(Some(2)).unwrap().profile.name, "warm");
}
#[test]
fn parallel_edits_are_serialized() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let store = store.clone();
            scope.spawn(move || {
                for _ in 0..10 {
                    store
                        .edit(None, |p| {
                            p.bypass = !p.bypass;
                            Ok(())
                        })
                        .unwrap();
                }
            });
        }
    });
    assert_eq!(store.load().unwrap().revision, 40);
    assert!(!store.load().unwrap().profile.bypass);
}
#[test]
fn only_one_audio_session_can_hold_the_lease() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let lease = store.session_lock().unwrap();
    assert!(store.session_lock().is_err());
    drop(lease);
    assert!(store.session_lock().is_ok());
}
#[test]
fn oversized_json_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.json");
    std::fs::write(&path, vec![b' '; 65537]).unwrap();
    assert!(read_json::<serde_json::Value>(&path).is_err());
}
