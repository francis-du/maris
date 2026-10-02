use maris::{mcp, store::Store};
use serde_json::{json, Value};
use std::io::Cursor;

fn check_references(value: &Value, root: &Value) -> usize {
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
                    .map(|v| check_references(v, root))
                    .sum::<usize>()
        }
        Value::Array(values) => values.iter().map(|v| check_references(v, root)).sum(),
        _ => 0,
    }
}

#[test]
fn dependency_updates_preserve_schema_dialect_and_nested_references() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"schema-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ].iter().map(Value::to_string).collect::<Vec<_>>().join("\n") + "\n";
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
            references += check_references(schema, schema);
        }
        assert!(references > 0, "schemas lost their nested types");
    }
    for tool in ["maris_schema", "maris_music_schema"] {
        let schema = mcp::invoke(&store, tool, json!({}), false).unwrap();
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert!(check_references(&schema, &schema) > 0);
    }
    assert!(!directory.path().join("control.json").exists());
}
