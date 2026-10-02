use super::*;

#[test]
fn build_resource_and_inventory_agree_without_user_cache() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::store::Store::at(directory.path());
    let inventory = research_models_at(&store).unwrap();
    let model = &inventory["models"][0];
    assert_eq!(model["id"], "musicnn");
    assert_eq!(
        model["bundled"].as_bool(),
        Some(musicnn_weights().is_some())
    );
    assert_eq!(model["downloadable"], false);
    assert_eq!(model["inference_available"], model["bundled"]);
    assert!(!directory.path().join("models").exists());
}

#[cfg(maris_musicnn_bundled)]
#[test]
fn bundled_weights_are_exactly_the_build_verified_payload() {
    use sha2::{Digest, Sha256};
    let bytes = musicnn_weights().expect("bundled build must contain weights");
    assert_eq!(bytes.len(), 3_175_212);
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        MUSICNN_SOURCE_SHA256
    );
    assert_eq!(
        MUSICNN_SOURCE_REVISION,
        "7cff1a4f9899825ddba77130899dfac4c8cfe9d5"
    );
}

#[cfg(not(maris_musicnn_bundled))]
#[test]
fn unbundled_build_contains_no_checkpoint_payload() {
    assert!(musicnn_weights().is_none());
    assert!(MUSICNN_MODEL_BYTES.is_empty());
}
