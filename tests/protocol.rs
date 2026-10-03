#[path = "support/live_telemetry.rs"]
mod live_telemetry;

use maris::{mcp, profile::Profile, store::Store};
use serde_json::{json, Value};
use std::io::Cursor;

fn check_schema_references(value: &Value, root: &Value) -> usize {
    match value {
        Value::Object(fields) => {
            let mut count = 0;
            if let Some(reference) = fields.get("$ref").and_then(Value::as_str) {
                let pointer = reference.strip_prefix('#').expect("local schema reference");
                assert!(
                    root.pointer(pointer).is_some(),
                    "unresolved schema reference: {reference}"
                );
                count += 1;
            }
            count
                + fields
                    .values()
                    .map(|value| check_schema_references(value, root))
                    .sum::<usize>()
        }
        Value::Array(values) => values
            .iter()
            .map(|value| check_schema_references(value, root))
            .sum(),
        _ => 0,
    }
}

#[test]
fn all_advertised_tool_schemas_keep_resolvable_nested_references() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"schema-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";
    for allow_write in [false, true] {
        let mut output = Vec::new();
        mcp::serve_io(
            &store,
            allow_write,
            &mut Cursor::new(&requests),
            &mut output,
        )
        .unwrap();
        let response: Value =
            serde_json::from_str(String::from_utf8(output).unwrap().lines().last().unwrap())
                .unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        let mut references = 0;
        for tool in tools {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "{}", tool["name"]);
            references += check_schema_references(schema, schema);
        }
        assert!(references > 0, "schemas lost their nested types");
    }
    for tool in ["maris_schema", "maris_music_schema"] {
        let schema = mcp::invoke(&store, tool, json!({}), false).unwrap();
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert!(check_schema_references(&schema, &schema) > 0);
    }
    assert!(!directory.path().join("control.json").exists());
}

#[test]
fn read_only_server_denies_writes() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let result = mcp::invoke(
        &store,
        "maris_preset",
        json!({"name":"warm","expected_revision":0}),
        false,
    );
    assert!(result.is_err());
    assert_eq!(store.load().unwrap().revision, 0);
}
#[test]
fn validation_has_no_side_effects() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let result = mcp::invoke(
        &store,
        "maris_validate",
        json!({"profile":Profile::preset("warm").unwrap()}),
        false,
    )
    .unwrap();
    assert_eq!(result["valid"], true);
    assert_eq!(store.load().unwrap().revision, 0);
}
#[test]
fn ai_writes_require_matching_revision() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    assert!(mcp::invoke(&store, "maris_preset", json!({"name":"warm"}), true).is_err());
    assert!(mcp::invoke(
        &store,
        "maris_preset",
        json!({"name":"warm","expected_revision":0}),
        true
    )
    .is_ok());
    assert!(mcp::invoke(
        &store,
        "maris_preset",
        json!({"name":"bass","expected_revision":0}),
        true
    )
    .is_err());
}
#[test]
fn handshake_and_notifications_produce_clean_json_lines() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"maris_status","arguments":{}}}),
    ];
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut output = Vec::new();
    mcp::serve_io(&store, false, &mut Cursor::new(input), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    let responses: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "maris");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let expected: std::collections::BTreeSet<_> = [
        "maris_listening",
        "maris_scenes",
        "maris_scene_preview",
        "maris_music_schema",
        "maris_status",
        "maris_platform",
        "maris_mixer",
        "maris_mixer_capabilities",
        "maris_presets",
        "maris_preset_info",
        "maris_schema",
        "maris_validate",
        "maris_models",
        "maris_analyze",
        "maris_music_context",
        "maris_device",
        "maris_device_bindings",
        "maris_applications",
        "maris_performance",
        "maris_tuning_suggest",
        "maris_autoeq_match",
        "maris_suggest",
    ]
    .into_iter()
    .collect();
    assert_eq!(names, expected);
    assert_eq!(responses[2]["result"]["isError"], false);
}
#[test]
fn writable_server_exposes_validated_new_write_tools() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let input = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";
    let mut output = Vec::new();
    mcp::serve_io(&store, true, &mut Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names: std::collections::BTreeSet<_> = responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for required in [
        "maris_tuning_apply",
        "maris_listening_undo",
        "maris_autoeq_bind",
        "maris_device_profile_bind",
        "maris_output_select",
        "maris_application_scope",
        "maris_music_apply",
        "maris_scene_apply",
        "maris_mixer_apply",
        "maris_mixer_undo",
    ] {
        assert!(names.contains(required), "missing writable tool {required}");
    }
    assert!(
        !names.contains("maris_model_fetch"),
        "Research acquisition must not be sold as a product tool"
    );
    let inventory = mcp::invoke(&store, "maris_models", json!({}), false).unwrap();
    assert_eq!(inventory["scope"], "shipped_backends");
    assert!(inventory["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["status"] != "candidate"));
}

#[test]
fn model_fetch_requires_write_permission_and_rejects_unreviewed_ids_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    assert!(mcp::invoke(&store, "maris_model_fetch", json!({"id":"musicnn"}), true).is_err());
    assert!(!directory.path().join("models").exists());
}

#[test]
fn live_routing_tools_require_write_permission_confirmation_and_active_native_session() {
    let _clock = maris::analysis::DebugClock::freeze_at(maris::analysis::now_ms());
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    store
        .write_json(
            "runtime.json",
            &json!({
                "active":true,
                "session_id":"native-session",
                "pid":9999,
                "system_backend":"coreaudio_process_tap",
                "updated_at_ms":maris::analysis::now_ms()
            }),
        )
        .unwrap();

    assert!(mcp::invoke(
        &store,
        "maris_output_select",
        json!({"output":null,"confirm_routing":true}),
        false
    )
    .is_err());
    assert!(mcp::invoke(
        &store,
        "maris_output_select",
        json!({"output":null,"confirm_routing":false}),
        true
    )
    .is_err());
    live_telemetry::refresh(&store);
    let queued = mcp::invoke(
        &store,
        "maris_output_select",
        json!({"output":null,"confirm_routing":true}),
        true,
    )
    .unwrap();
    assert_eq!(queued["queued"], true);
    assert_eq!(queued["changes_system_default_output"], false);

    live_telemetry::refresh(&store);
    assert!(mcp::invoke(
        &store,
        "maris_application_scope",
        json!({"pids":[9999],"confirm_routing":true}),
        true
    )
    .is_err());
    live_telemetry::refresh(&store);
    assert!(mcp::invoke(
        &store,
        "maris_application_scope",
        json!({"pids":[123,456],"confirm_routing":true}),
        true
    )
    .is_err());
    // Only the engine's consumption opens the single pending-command slot.
    std::fs::remove_file(directory.path().join("control.json")).unwrap();
    live_telemetry::refresh(&store);
    let queued = mcp::invoke(
        &store,
        "maris_application_scope",
        json!({"pids":[123,456],"confirm_routing":true}),
        true,
    )
    .unwrap();
    assert_eq!(queued["pids"], json!([123, 456]));
    let command: Value = maris::store::read_json(&directory.path().join("control.json")).unwrap();
    assert_eq!(command["action"], "select_applications");
}

#[test]
fn scene_tools_preview_without_writes_and_apply_with_exact_listening_revision() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let catalog = mcp::invoke(&store, "maris_scenes", json!({}), false).unwrap();
    assert_eq!(
        catalog["count"].as_u64(),
        Some(maris::scenes::SCENES.len() as u64)
    );
    let args = json!({"name":"focus","device":"Headphones"});
    let preview = mcp::invoke(&store, "maris_scene_preview", args.clone(), false).unwrap();
    assert_eq!(preview["applied"], false);
    assert!(!directory.path().join("listening.json").exists());
    assert!(mcp::invoke(&store, "maris_scene_apply", args, true).is_err());
    let args = json!({"name":"focus","device":"Headphones","expected_listening_revision":0});
    assert!(mcp::invoke(&store, "maris_scene_apply", args.clone(), false).is_err());
    let applied = mcp::invoke(&store, "maris_scene_apply", args.clone(), true).unwrap();
    assert_eq!(applied["revision"], 1);
    assert_eq!(
        applied["devices"]["Headphones"],
        preview["preview"]["profile"]
    );
    assert!(mcp::invoke(&store, "maris_scene_apply", args, true).is_err());
    assert_eq!(store.load().unwrap().revision, 0);
}

#[test]
fn malformed_and_oversized_input_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let mut output = Vec::new();
    mcp::serve_io(&store, true, &mut Cursor::new("{bad}\n"), &mut output).unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["error"]["code"], -32700);
    output.clear();
    mcp::serve_io(
        &store,
        true,
        &mut Cursor::new(vec![b'x'; 70000]),
        &mut output,
    )
    .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["error"]["code"], -32600);
    assert_eq!(store.load().unwrap().revision, 0);
}
