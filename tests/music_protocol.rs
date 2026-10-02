use maris::{listening, mcp, music::MusicProfile, store::Store};
use serde_json::json;
#[test]
fn music_agent_writes_require_permission_and_listening_revision() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let args = json!({"profile":MusicProfile::preset("warm").unwrap(),"device":"Headphones","expected_listening_revision":0});
    assert!(mcp::invoke(&store, "maris_music_apply", args.clone(), false).is_err());
    assert!(mcp::invoke(&store, "maris_music_apply", args.clone(), true).is_ok());
    assert!(mcp::invoke(&store, "maris_music_apply", args, true).is_err());
    assert_eq!(listening::load(&store).unwrap().revision, 1);
    assert_eq!(store.load().unwrap().revision, 0);
}
#[test]
fn music_agent_reads_never_start_an_audio_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let status = mcp::invoke(&store, "maris_listening", json!({}), false).unwrap();
    assert_eq!(status["listening"]["revision"], 0);
    assert!(!temp.path().join("runtime.json").exists());
    assert!(mcp::invoke(
        &store,
        "maris_music_schema",
        json!({"unexpected":true}),
        false
    )
    .is_err());
}
