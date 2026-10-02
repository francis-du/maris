use super::*;
use crate::audio::pulse::server::{Sink, SocketId};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
};

struct Fixture {
    directory: tempfile::TempDir,
    _socket: UnixListener,
    server: Server,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let socket = UnixListener::bind(directory.path().join("native")).unwrap();
        let script = directory.path().join("pactl");
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/pulse_fixture.py"),
            &script,
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let server = Server::fixture(
            SocketId::read(&directory.path().join("native")).unwrap(),
            script,
        );
        let store = Store::at(directory.path().join("state"));
        let this = Self {
            directory,
            _socket: socket,
            server,
            store,
        };
        this.write(json!({"default":"speakers","sinks":[
            {"index":1,"name":"speakers","description":"Speakers","monitor_source_name":"speakers.monitor","owner_module":null,"flags":["LATENCY"],"properties":{"device.api":"alsa"}},
            {"index":2,"name":"maris_fixture","description":"Maris","monitor_source_name":"maris_fixture.monitor","owner_module":9,"flags":[],"properties":{"device.class":"abstract"}},
            {"index":3,"name":"headphones","description":"Headphones","monitor_source_name":"headphones.monitor","owner_module":null,"flags":["LATENCY"],"properties":{"device.api":"bluez5"}}],
            "inputs":[{"index":10,"sink":1,"client":20,"properties":{"application.process.id":"123","object.serial":"99","application.id":"fixture.player"}},
                {"index":11,"sink":1,"client":21,"properties":{"application.id":"audio.maris.app","media.name":"fixture"}}],
            "modules":[{"index":9,"name":"module-null-sink","argument":"sink_name=maris_fixture rate=48000"}]}));
        this
    }
    fn read(&self) -> Value {
        serde_json::from_slice(&fs::read(self.directory.path().join("fixture.json")).unwrap())
            .unwrap()
    }
    fn write(&self, value: Value) {
        fs::write(
            self.directory.path().join("fixture.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    fn guard(&self) -> Guard {
        Guard {
            server: self.server.clone(),
            store: self.store.clone(),
            watcher: None,
            pids: BTreeMap::new(),
            exemptions: Vec::new(),
            active: false,
            journal: Journal {
                version: 1,
                token: "fixture".into(),
                socket: self.server.socket.clone(),
                sink: "maris_fixture".into(),
                module: Some(9),
                original_default: "speakers".into(),
                routes: Vec::new(),
            },
        }
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.directory.path().join("calls.jsonl")).unwrap_or_default()
    }
}

#[test]
fn redirects_application_after_journaling_and_never_captures_own_output() {
    let f = Fixture::new();
    let mut guard = f.guard();
    guard.reconcile().unwrap();
    assert_eq!(f.read()["inputs"][0]["sink"], 2);
    assert_eq!(f.read()["inputs"][1]["sink"], 1);
    assert_eq!(guard.journal.routes.len(), 1);
    let record = record_name(&guard.journal.token).unwrap();
    assert!(f.store.directory.join(&record).is_file());
    assert!(!f.calls().contains("volume"));
    assert!(!f.calls().contains("set-default"));
    finish_recovery(&f.store, &record, &guard.journal, &f.server).unwrap();
    assert_eq!(f.read()["inputs"][0]["sink"], 1);
    assert!(f.read()["modules"].as_array().unwrap().is_empty());
    assert!(!f.store.directory.join(&record).exists());
}

#[test]
fn recovery_preserves_manual_routes_and_does_not_follow_reused_stream_ids() {
    for manual in [true, false] {
        let f = Fixture::new();
        let mut guard = f.guard();
        guard.reconcile().unwrap();
        let mut state = f.read();
        if manual {
            state["inputs"][0]["sink"] = json!(3);
        } else {
            state["inputs"][0]["client"] = json!(44);
        }
        f.write(state.clone());
        let record = record_name(&guard.journal.token).unwrap();
        finish_recovery(&f.store, &record, &guard.journal, &f.server).unwrap();
        assert_eq!(f.read()["inputs"][0], state["inputs"][0]);
    }
}

#[test]
fn manual_routing_is_not_immediately_taken_back_by_refresh() {
    let f = Fixture::new();
    let mut guard = f.guard();
    guard.reconcile().unwrap();
    let mut state = f.read();
    state["inputs"][0]["sink"] = json!(3);
    f.write(state);
    guard.reconcile().unwrap();
    guard.reconcile().unwrap();
    assert_eq!(f.read()["inputs"][0]["sink"], 3);
    assert!(guard.journal.routes.is_empty());
}

#[test]
fn a_failed_move_keeps_a_recovery_record_and_does_not_change_defaults() {
    let f = Fixture::new();
    let mut guard = f.guard();
    let mut state = f.read();
    state["fail_move"] = json!(true);
    f.write(state);
    assert!(guard.reconcile().is_err());
    let record = record_name(&guard.journal.token).unwrap();
    let journal: Journal = store::read_json(&f.store.directory.join(&record)).unwrap();
    assert_eq!(journal.routes.len(), 1);
    assert_eq!(f.read()["inputs"][0]["sink"], 1);
    finish_recovery(&f.store, &record, &journal, &f.server).unwrap();
    assert_eq!(f.read()["default"], "speakers");
}

#[test]
fn disappearing_original_output_restores_to_current_physical_default() {
    let f = Fixture::new();
    let mut guard = f.guard();
    guard.reconcile().unwrap();
    let mut state = f.read();
    state["default"] = json!("headphones");
    state["sinks"]
        .as_array_mut()
        .unwrap()
        .retain(|s| s["name"] != "speakers");
    f.write(state);
    let record = record_name(&guard.journal.token).unwrap();
    finish_recovery(&f.store, &record, &guard.journal, &f.server).unwrap();
    assert_eq!(f.read()["inputs"][0]["sink"], 3);
}

#[test]
fn unrelated_module_is_never_unloaded_when_an_id_is_reused() {
    let f = Fixture::new();
    let mut guard = f.guard();
    guard.reconcile().unwrap();
    let mut state = f.read();
    state["modules"][0]["name"] = json!("module-loopback");
    f.write(state);
    let record = record_name(&guard.journal.token).unwrap();
    assert!(finish_recovery(&f.store, &record, &guard.journal, &f.server).is_err());
    assert!(!f.calls().contains("unload-module"));
    assert_eq!(
        f.read()["inputs"][0]["sink"],
        2,
        "Unowned recovery cannot move a stream"
    );
    assert!(f.store.directory.join(&record).is_file());
}

#[test]
fn output_selection_rejects_private_sink_and_ambiguous_device_labels() {
    let f = Fixture::new();
    let mut snapshot = f.server.snapshot().unwrap();
    assert_eq!(
        snapshot.output(Some("pulse:headphones")).unwrap().name,
        "headphones"
    );
    assert!(snapshot.output(Some("pulse:maris_fixture")).is_err());
    let other: Sink =
        serde_json::from_value(json!({"index":8,"name":"other","description":"Headphones",
        "monitor_source_name":"other.monitor","flags":[],"properties":{"device.api":"alsa"}}))
        .unwrap();
    snapshot.sinks.push(other);
    assert!(snapshot.output(Some("Headphones")).is_err());
    assert_eq!(snapshot.output(Some("pulse:headphones")).unwrap().index, 3);
}

#[test]
fn flat_volume_or_unknown_volume_policy_blocks_output_and_application_moves() {
    for flags in [
        json!(["LATENCY", "FLAT_VOLUME"]),
        json!("LATENCY FLAT_VOLUME"),
        Value::Null,
    ] {
        let f = Fixture::new();
        let mut state = f.read();
        state["sinks"][0]["flags"] = flags;
        f.write(state);
        assert!(f.server.snapshot().unwrap().output(None).is_err());
        let mut guard = f.guard();
        guard.reconcile().unwrap();
        assert_eq!(f.read()["inputs"][0]["sink"], 1);
        assert!(guard.journal.routes.is_empty());
    }
}

#[test]
fn graph_subscription_ignores_client_churn_from_read_only_queries() {
    use crate::audio::pulse::server::graph_event;
    assert!(graph_event("Event 'change' on sink #1"));
    assert!(graph_event("Event 'new' on sink-input #2"));
    assert!(graph_event("Event 'change' on server #0"));
    assert!(!graph_event("Event 'new' on client #3"));
    assert!(!graph_event("Event 'remove' on client #3"));
}

#[test]
fn remote_server_and_unsafe_endpoint_names_are_not_accepted() {
    assert!(SocketId::read(std::path::Path::new("tcp:example.test:4713")).is_err());
    for name_value in [
        "",
        "../speaker",
        "headphones;touch",
        "x\ny",
        "--server=tcp:remote",
    ] {
        assert!(name(name_value).is_err());
    }
    let f = Fixture::new();
    let command = f
        .server
        .stream_command(false, "speakers", "fixture")
        .unwrap();
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args[0].starts_with("--server=unix:"));
    assert!(args
        .iter()
        .any(|arg| arg == "--property=application.id=audio.maris.app"));
}
