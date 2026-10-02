use serde_json::Value;
use std::process::Command;

#[test]
fn model_cli_has_no_runtime_fetch_or_import_path() {
    let directory = tempfile::tempdir().unwrap();
    for action in ["fetch", "import"] {
        let output = Command::new(env!("CARGO_BIN_EXE_maris"))
            .env("MARIS_STATE_DIR", directory.path())
            .args(["--json", "models", action, "musicnn"])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    assert!(!directory.path().join("models").exists());
}

#[test]
fn provenance_inventory_reports_build_resource_without_runtime_download() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_maris"))
        .env("MARIS_STATE_DIR", directory.path())
        .args(["--json", "models", "research"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let model = &value["models"][0];
    assert_eq!(model["id"], "musicnn");
    assert_eq!(model["downloadable"], false);
    assert_eq!(model["inference_available"], model["bundled"]);
    assert!(!directory.path().join("models").exists());
}
