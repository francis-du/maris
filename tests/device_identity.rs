use maris::{device_identity, store::Store};

fn coreaudio(name: &str, stable_id: Option<&str>) -> device_identity::Identity {
    device_identity::Identity {
        platform: "macos".into(),
        stable_id: stable_id.map(str::to_owned),
        model_id: Some("model.fixture".into()),
        display_name: name.into(),
        source: if stable_id.is_some() {
            "coreaudio_device_uid".into()
        } else {
            "display_name".into()
        },
    }
}

#[test]
fn stable_id_survives_display_name_changes_without_merging_devices() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());

    let first =
        device_identity::resolve_or_remember(&store, coreaudio("Studio Headphones", Some("uid-A")))
            .unwrap();
    assert_eq!(first.profile_key, "Studio Headphones");
    assert_eq!(first.binding_source, "stable_id");
    assert_eq!(first.binding_revision, 1);

    let unchanged =
        device_identity::resolve_or_remember(&store, coreaudio("Studio Headphones", Some("uid-A")))
            .unwrap();
    assert_eq!(unchanged.binding_revision, 1);

    let renamed =
        device_identity::resolve_or_remember(&store, coreaudio("My Headphones", Some("uid-A")))
            .unwrap();
    assert_eq!(renamed.profile_key, "Studio Headphones");
    assert_eq!(renamed.binding_revision, 2);

    let other =
        device_identity::resolve_or_remember(&store, coreaudio("My Headphones", Some("uid-B")))
            .unwrap();
    assert_eq!(other.profile_key, "My Headphones");
    assert_ne!(other.profile_key, renamed.profile_key);
}

#[test]
fn same_display_name_with_different_stable_ids_gets_distinct_profile_keys() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let first =
        device_identity::resolve_or_remember(&store, coreaudio("Headphones", Some("device-AAAA")))
            .unwrap();
    let second =
        device_identity::resolve_or_remember(&store, coreaudio("Headphones", Some("device-BBBB")))
            .unwrap();
    assert_eq!(first.profile_key, "Headphones");
    assert_ne!(second.profile_key, first.profile_key);
    assert!(second.profile_key.starts_with("Headphones @"));
    let repeated =
        device_identity::resolve_or_remember(&store, coreaudio("Headphones", Some("device-BBBB")))
            .unwrap();
    assert_eq!(repeated.profile_key, second.profile_key);
}

#[test]
fn display_name_fallback_does_not_persist_fake_stable_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let resolution =
        device_identity::resolve_or_remember(&store, coreaudio("Generic Output", None)).unwrap();
    assert_eq!(resolution.profile_key, "Generic Output");
    assert_eq!(resolution.binding_source, "display_name");
    assert_eq!(device_identity::load(&store).unwrap().bindings.len(), 0);
}

#[test]
fn explicit_profile_rebind_is_revision_checked() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let resolution =
        device_identity::resolve_or_remember(&store, coreaudio("Headphones", Some("uid-A")))
            .unwrap();
    assert_eq!(resolution.binding_revision, 1);

    assert!(device_identity::bind_profile_key(&store, Some(0), "uid-A", "Reference").is_err());
    assert!(device_identity::bind_profile_key(&store, Some(1), "uid-A", "Reference").is_err());
    maris::listening::edit(&store, None, Some("Reference"), |_| Ok(())).unwrap();
    let rebound = device_identity::bind_profile_key(&store, Some(1), "uid-A", "Reference").unwrap();
    assert_eq!(rebound.revision, 2);
    assert_eq!(rebound.bindings["uid-A"].profile_key, "Reference");
}
