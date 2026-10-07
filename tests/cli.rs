use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn run(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_maris"))
        .env("MARIS_STATE_DIR", directory)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn help_exposes_all_public_commands_with_descriptions() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    let commands = [
        "app",
        "package",
        "update",
        "language",
        "sound",
        "mixer",
        "devices",
        "applications",
        "application",
        "models",
        "analyze",
        "smart",
        "smart-apply",
        "enhance",
        "voice",
        "start",
        "stop",
        "tray",
        "doctor",
        "status",
        "presets",
        "preset",
        "band",
        "preamp",
        "bypass",
        "crossfeed",
        "width",
        "undo",
        "schema",
        "apply",
        "export",
        "render",
        "run",
        "play",
        "tui",
        "system",
        "restore",
        "mcp",
        "integration",
    ];
    for command in commands {
        let line = help
            .lines()
            .find(|line| line.split_whitespace().next() == Some(command))
            .unwrap_or_else(|| panic!("missing command in help: {command}"));
        assert!(
            line.split_whitespace().count() >= 2,
            "command has no help description: {line:?}"
        );
        let detail = run(directory.path(), &[command, "--help"]);
        assert!(detail.status.success(), "{command} --help failed");
        assert!(String::from_utf8(detail.stdout).unwrap().contains("Usage:"));
    }
    assert!(!help.contains("__route-watch"));
}

#[test]
fn nested_sound_and_mixer_commands_have_descriptions() {
    let directory = tempfile::tempdir().unwrap();
    for (parent, commands) in [
        (
            "sound",
            &[
                "status",
                "capability",
                "match",
                "bind",
                "schema",
                "scenes",
                "preview",
                "preset",
                "set",
                "compare",
                "save-device",
                "import",
                "apply",
            ][..],
        ),
        (
            "mixer",
            &[
                "status",
                "capabilities",
                "run",
                "eq",
                "band",
                "compressor",
                "add",
                "remove",
                "set",
                "bus",
                "duck",
                "scene-save",
                "scene-restore",
                "undo",
            ][..],
        ),
    ] {
        let output = run(directory.path(), &[parent, "--help"]);
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        for command in commands {
            let line = help
                .lines()
                .find(|line| line.split_whitespace().next() == Some(*command))
                .unwrap_or_else(|| panic!("missing {parent} command: {command}"));
            assert!(
                line.split_whitespace().count() >= 2,
                "missing description: {line:?}"
            );
        }
    }
}

#[test]
fn update_from_a_development_binary_fails_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json", "update", "--dry-run"]);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("installed CLI release") || message.contains("updater"));
}
#[test]
fn negative_gains_and_explicit_boolean_values_parse() {
    let directory = tempfile::tempdir().unwrap();
    assert!(run(directory.path(), &["--json", "band", "1", "-3"])
        .status
        .success());
    assert!(run(directory.path(), &["--json", "bypass", "true"])
        .status
        .success());
    assert!(run(directory.path(), &["--json", "bypass", "false"])
        .status
        .success());
    let output = run(directory.path(), &["--json", "status"]);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["state"]["profile"]["bypass"], false);
    assert_eq!(value["state"]["profile"]["bands"][0]["gain_db"], -3.0);
}
#[test]
fn invalid_mutation_reports_json_and_nonzero_exit() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json", "band", "99", "3"]);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], false);
}
#[test]
fn system_capture_requires_explicit_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json", "system"]);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("accept-routing"));
}
#[test]
fn model_listing_is_read_only_and_has_no_runtime_acquisition_command() {
    let directory = tempfile::tempdir().unwrap();
    let listed = run(directory.path(), &["--json", "models"]);
    assert!(listed.status.success());
    let value: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(value["scope"], "shipped_backends");
    assert_eq!(value["music_requires_model_download"], false);
    assert_eq!(value["models"].as_array().unwrap().len(), 3);
    let musicnn = value["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == "musicnn")
        .unwrap();
    assert!(musicnn["bundled"].is_boolean());
    assert_eq!(musicnn["inference_available"], musicnn["bundled"]);
    assert_eq!(musicnn["downloadable"], false);
    let research = run(directory.path(), &["--json", "models", "research"]);
    assert!(research.status.success());
    let research: Value = serde_json::from_slice(&research.stdout).unwrap();
    assert_eq!(research["scope"], "model_provenance");
    assert_eq!(research["models"].as_array().unwrap().len(), 1);
    assert_eq!(research["models"][0]["id"], "musicnn");
    assert_eq!(
        research["models"][0]["inference_available"],
        research["models"][0]["bundled"]
    );
    assert_eq!(research["models"][0]["downloadable"], false);
    assert!(!directory.path().join("models").exists());

    let help = run(directory.path(), &["models", "--help"]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(!help.contains("fetch"));
    assert!(!help.contains("import"));
}

#[test]
fn application_metadata_read_does_not_start_capture() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json", "applications"]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["available"].is_boolean());
    if cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    )) && value["available"] == true
    {
        assert_eq!(value["per_application_capture"], true);
        assert_eq!(value["per_application_routing"], true);
    } else {
        assert_eq!(value["per_application_routing"], false);
    }
    assert!(!directory.path().join("runtime.json").exists());
}

#[test]
fn application_capture_requires_explicit_routing_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json", "application", "--pid", "1"]);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("accept-routing"));
    assert!(!directory.path().join("runtime.json").exists());
}

#[test]
fn preset_catalog_lists_shows_applies_and_restores_with_stable_json() {
    let directory = tempfile::tempdir().unwrap();
    let listed = run(directory.path(), &["--json", "presets"]);
    assert!(listed.status.success());
    let catalog: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(catalog["count"], 27);
    assert!(catalog["presets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|preset| preset["id"] == "eqmac:acoustic" && preset["category"] == "eqmac"));

    let shown = run(
        directory.path(),
        &["--json", "presets", "show", "eqmac:acoustic"],
    );
    assert!(shown.status.success());
    let detail: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(
        detail["source_commit"],
        "04e5a3a9bd3a65f2b5105cf54a76d2c72a1d00d7"
    );
    assert_eq!(detail["source_gains_db"][4], 18.22);
    assert!(detail["safe_preamp_db"].as_f64().unwrap() < -10.0);

    let applied = run(
        directory.path(),
        &["--json", "presets", "apply", "eqmac:bass-reducer"],
    );
    assert!(applied.status.success());
    let state: Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(state["state"]["profile"]["name"], "eqmac:bass-reducer");
    assert_eq!(
        state["state"]["profile"]["bands"][0]["bandwidth_octaves"],
        0.5
    );

    let restored = run(directory.path(), &["--json", "presets", "restore"]);
    assert!(restored.status.success());
    let restored: Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["profile"]["name"], "flat");
}

#[test]
fn unknown_preset_returns_structured_error_without_mutating_state() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(
        directory.path(),
        &["--json", "presets", "apply", "eqmac:not-real"],
    );
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "PRESET_NOT_FOUND");
    let status = run(directory.path(), &["--json", "status"]);
    let state: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(state["state"]["revision"], 0);
}

#[test]
fn scene_cli_keeps_machine_fields_english_and_preview_read_only_in_every_locale() {
    for language in maris::i18n::LANGUAGES {
        let directory = tempfile::tempdir().unwrap();
        let listed = run(
            directory.path(),
            &["--lang", language, "--json", "sound", "scenes"],
        );
        assert!(
            listed.status.success(),
            "{}",
            String::from_utf8_lossy(&listed.stdout)
        );
        let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
        assert_eq!(
            listed["count"].as_u64(),
            Some(maris::scenes::SCENES.len() as u64)
        );
        assert_eq!(listed["scenes"][0]["id"], "focus");
        let output = run(
            directory.path(),
            &[
                "--lang",
                language,
                "--json",
                "sound",
                "--device",
                "Fixture Headphones",
                "preview",
                "dialogue",
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(preview["applied"], false);
        assert_eq!(preview["expected_listening_revision"], 0);
        assert!(!directory.path().join("listening.json").exists());
        let applied = run(
            directory.path(),
            &[
                "--lang",
                language,
                "--json",
                "--expected-revision",
                "0",
                "sound",
                "--device",
                "Fixture Headphones",
                "preset",
                "dialogue",
            ],
        );
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stdout)
        );
        let applied: Value = serde_json::from_slice(&applied.stdout).unwrap();
        assert_eq!(applied["revision"], 1);
        assert_eq!(
            applied["devices"]["Fixture Headphones"],
            preview["preview"]["profile"]
        );
        assert!(!directory.path().join("runtime.json").exists());
        assert!(!directory.path().join("control.json").exists());
    }
}

#[test]
fn empty_locale_environment_does_not_hide_the_next_preference() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_maris"))
        .env("MARIS_STATE_DIR", directory.path())
        .env("MARIS_LANG", "")
        .env("LC_ALL", "")
        .env("LC_MESSAGES", "")
        .env("LANG", "ja_JP.UTF-8")
        .args(["--json", "language"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["language"], "ja");
}

#[test]
fn wav_render_uses_the_same_dsp_without_overwriting_files() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.wav");
    let output = directory.path().join("output.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&input, spec).unwrap();
    for i in 0..1000 {
        writer.write_sample((i % 100 * 100) as i16).unwrap();
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
    let args = [
        "--json",
        "render",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ];
    let rendered = run(directory.path(), &args);
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stdout)
    );
    assert_eq!(hound::WavReader::open(&output).unwrap().len(), 2000);
    assert!(!run(directory.path(), &args).status.success());
}
