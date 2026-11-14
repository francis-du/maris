use crate::{
    audio,
    control::store::{Store, MAX_JSON_BYTES},
    dsp::profile::Profile,
    models as neural, presets,
    tuning::eq as smart,
    tuning::planner as music_tuning,
};
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

mod mcp_invoke;
pub use mcp_invoke::invoke;

fn tool(name: &str, description: &str, schema: Value, readonly: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":schema,
        "annotations":{"readOnlyHint":readonly,"destructiveHint":false,"idempotentHint":readonly,"openWorldHint":false}})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn profile_arguments(mutation: bool) -> Value {
    let mut profile = serde_json::to_value(
        schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Profile>(),
    )
    .expect("Schema is serializable");
    let definitions = profile
        .as_object_mut()
        .and_then(|v| v.remove("definitions"))
        .unwrap_or_else(|| json!({}));
    if let Some(map) = profile.as_object_mut() {
        map.remove("$schema");
    }
    let mut schema = if mutation {
        object(
            json!({"profile":profile,"expected_revision":{"type":"integer","minimum":0}}),
            &["profile", "expected_revision"],
        )
    } else {
        object(json!({"profile":profile}), &["profile"])
    };
    schema["definitions"] = definitions;
    schema
}
fn suggestion_arguments() -> Value {
    let mut proposal = serde_json::to_value(
        schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<smart::Proposal>(),
    )
    .expect("Schema is serializable");
    let definitions = proposal
        .as_object_mut()
        .and_then(|v| v.remove("definitions"))
        .unwrap_or_else(|| json!({}));
    if let Some(map) = proposal.as_object_mut() {
        map.remove("$schema");
    }
    let mut schema = object(json!({"proposal":proposal}), &["proposal"]);
    schema["definitions"] = definitions;
    schema
}
fn music_tuning_arguments() -> Value {
    let mut proposal = serde_json::to_value(
        schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<music_tuning::Proposal>(),
    )
    .expect("Context tuning schema is serializable");
    let definitions = proposal
        .as_object_mut()
        .and_then(|value| value.remove("definitions"))
        .unwrap_or_else(|| json!({}));
    if let Some(value) = proposal.as_object_mut() {
        value.remove("$schema");
    }
    let mut schema = object(json!({"proposal":proposal}), &["proposal"]);
    schema["definitions"] = definitions;
    schema
}
fn music_arguments() -> Value {
    let mut profile = serde_json::to_value(
        schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<crate::dsp::music::MusicProfile>(),
    )
    .expect("Music schema is serializable");
    let definitions = profile
        .as_object_mut()
        .and_then(|p| p.remove("definitions"))
        .unwrap_or_else(|| json!({}));
    if let Some(p) = profile.as_object_mut() {
        p.remove("$schema");
    }
    let mut schema = object(
        json!({"profile":profile,"device":{"type":"string"},"expected_listening_revision":{"type":"integer","minimum":0}}),
        &["profile", "device", "expected_listening_revision"],
    );
    schema["definitions"] = definitions;
    schema
}
fn mixer_arguments() -> Value {
    let mut config = serde_json::to_value(
        schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<crate::mixer::MixerConfig>(),
    )
    .expect("Mixer schema is serializable");
    let definitions = config
        .as_object_mut()
        .and_then(|value| value.remove("definitions"))
        .unwrap_or_else(|| json!({}));
    if let Some(value) = config.as_object_mut() {
        value.remove("$schema");
    }
    let mut schema = object(
        json!({"config":config,"expected_mixer_revision":{"type":"integer","minimum":0}}),
        &["config", "expected_mixer_revision"],
    );
    schema["definitions"] = definitions;
    schema
}
fn preset_name_schema() -> Value {
    json!({"type":"string","enum":presets::ids()})
}
fn tools(allow_write: bool) -> Value {
    let mut list = vec![
        tool("maris_listening", "Read device-specific music preferences and their separate revision. Does not start capture.", object(json!({}), &[]), true),
        tool("maris_scenes", "List original listening scene preferences. These are not device measurements and require no model download.", object(json!({}), &[]), true),
        tool("maris_scene_preview", "Preview a listening preset or scene against a saved device profile and capability limits without writing state. Use default to target fallback preferences.", object(json!({"name":{"type":"string"},"device":{"type":"string"}}), &["name","device"]), true),
        tool("maris_music_schema", "Read the accepted sound and parametric-EQ settings and their limits. Device measurements must come from a known source.", object(json!({}), &[]), true),
        tool("maris_status", "Read current settings, their revision, and audio status. Does not start recording.", object(json!({}), &[]), true),
        tool("maris_platform", "Read the audio functions supported by this build and platform, including unavailable functions.", object(json!({}), &[]), true),
        tool("maris_mixer", "Read saved mixer channels, their two output assignments, and supported audio functions. Does not open devices.", object(json!({}), &[]), true),
        tool("maris_mixer_capabilities", "Read which mixer, application-audio and device functions this build implements. This is not a hardware test.", object(json!({}), &[]), true),
        tool("maris_presets", "List the unified built-in preset catalog, including Maris and attributed eqMac open-source presets. Does not start audio.", object(json!({}), &[]), true),
        tool("maris_preset_info", "Inspect one preset's source curve, runtime Q conversion, and conservative safety preamp without applying it.", object(json!({"name":preset_name_schema()}), &["name"]), true),
        tool("maris_schema", "Read the profile JSON Schema. Numeric bounds are enforced by maris_validate and all writes.", object(json!({}), &[]), true),
        tool("maris_validate", "Validate a proposed profile without applying it; returns conservative digital headroom.", profile_arguments(false), true),
        tool("maris_models", "List the included audio-analysis and recognition models and whether each is available. Speech noise reduction is separate and must be enabled explicitly.", object(json!({}),&[]), true),
        tool("maris_analyze", "Read recent pre-EQ signal statistics from an existing audio session. No raw audio is returned or capture started.",object(json!({}),&[]),true),
        tool("maris_music_context", "Read recent music analysis and recognized tags. Empty tags mean recognition is unavailable or uncertain. Never returns raw audio.",object(json!({}),&[]),true),
        tool("maris_device", "Read current output identity, stable profile binding, capability, correction and known physical limits. Unknown limits remain unknown.",object(json!({}),&[]),true),
        tool("maris_device_bindings", "Read stable device-ID to listening-profile bindings. Display-name fallback remains explicit when no stable ID is available.",object(json!({}),&[]),true),
        tool("maris_applications", "Read audio-process metadata and platform capability flags. This read-only tool never starts or changes application capture.",object(json!({}),&[]),true),
        tool("maris_performance", "Read audio buffer interruptions, model latency and measured callback time. Worker CPU share is derived from measured worker processing counters and is null until worker frames exist.",object(json!({}),&[]),true),
        tool("maris_tuning_suggest", "Preview tuning for the selected device using recent audio analysis, available music recognition, correction settings and listening preferences.",object(json!({"goal":{"type":"string","enum":music_tuning::GOALS}}),&["goal"]),true),
        tool("maris_autoeq_match", "Match an exact output name against the locally cached pinned AutoEq index only. This tool does not access the network.",object(json!({"device":{"type":"string"}}),&["device"]),true),
        tool("maris_suggest", "Preview the legacy conservative signal-aware ten-band EQ proposal. No settings are changed.",object(json!({"goal":{"type":"string","enum":smart::GOALS}}),&["goal"]),true),
    ];
    if allow_write {
        list.extend([
            tool("maris_scene_apply", "Apply a listening preset or scene using the same revision-checked transaction as the TUI/CLI. Preserves correction and does not change routing or system volume.", object(json!({"name":{"type":"string"},"device":{"type":"string"},"expected_listening_revision":{"type":"integer","minimum":0}}), &["name","device","expected_listening_revision"]), false),
            tool("maris_music_apply", "Apply a device-specific music profile with its listening revision. Use an exact output name; 'default' explicitly edits the fallback. Does not start audio or override OS volume.", music_arguments(), false),
            tool("maris_tuning_apply", "Apply a previously previewed context-aware music tuning proposal through listening revision validation. Correction filters cannot be changed by this path.", music_tuning_arguments(), false),
            tool("maris_listening_undo", "Restore the previous device-listening library while keeping the listening revision monotonic.", object(json!({"expected_listening_revision":{"type":"integer","minimum":0}}), &["expected_listening_revision"]), false),
            tool("maris_autoeq_bind", "Bind a device to an exact model from the locally cached pinned AutoEq index and cached profile. No network access occurs; refresh/cache must happen separately.", object(json!({"device":{"type":"string"},"model":{"type":"string"}}), &["device","model"]), false),
            tool("maris_device_profile_bind", "Explicitly bind a connected stable device ID to an existing listening-profile key with device-binding revision checking.", object(json!({"stable_id":{"type":"string"},"profile_key":{"type":"string"},"expected_device_binding_revision":{"type":"integer","minimum":0}}), &["stable_id","profile_key","expected_device_binding_revision"]), false),
            tool("maris_output_select", "Request an output change for active system audio on a supported platform. Use a current uid:, pulse: or wasapi: device ID; null follows the system default. Requires confirmation and never changes the OS default output.", object(json!({"output":{"anyOf":[{"type":"string","minLength":1,"maxLength":512},{"type":"null"}]},"confirm_routing":{"type":"boolean","const":true}}), &["output","confirm_routing"]), false),
            tool("maris_application_scope", "Choose which apps Maris processes during active system audio. Empty pids selects all supported system playback except Maris. Apps may be muted or redirected while captured; explicit routing confirmation is required.", object(json!({"pids":{"type":"array","minItems":0,"maxItems":64,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":2147483647}},"confirm_routing":{"type":"boolean","const":true}}), &["pids","confirm_routing"]), false),
            tool("maris_mixer_apply", "Save mixer settings only when the supplied revision is current. A running mixer applies level and effect changes; changed input or output assignments require a restart. Does not start audio.", mixer_arguments(), false),
            tool("maris_mixer_undo", "Restore the previous mixer control configuration with mixer-revision checking.", object(json!({"expected_mixer_revision":{"type":"integer","minimum":0}}), &["expected_mixer_revision"]), false),
            tool("maris_apply", "Apply a validated profile to the running audio session. Read current revision first; do not claim acoustic measurements or hearing protection.", profile_arguments(true), false),
            tool("maris_preset", "Apply a built-in preset with optimistic revision checking and the same DSP/safety conversion as the CLI/TUI.", object(json!({"name":preset_name_schema(),"expected_revision":{"type":"integer","minimum":0}}), &["name","expected_revision"]), false),
            tool("maris_undo", "Swap the current and previous profile. Requires current revision.", object(json!({"expected_revision":{"type":"integer","minimum":0}}), &["expected_revision"]), false),
            tool("maris_apply_suggestion", "Apply a previously previewed smart proposal. Rejects stale revisions, expired proposals, and modified profile payloads.",suggestion_arguments(),false),
        ]);
    }
    json!({"tools":list})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    profile: Profile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    profile: Profile,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    expected_revision: u64,
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub fn handle(store: &Store, message: Value, allow_write: bool, phase: &mut u8) -> Option<Value> {
    let id = message.get("id").cloned();
    if !message.is_object()
        || message.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || message.get("method").and_then(Value::as_str).is_none()
        || id
            .as_ref()
            .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
    {
        return Some(error(Value::Null, -32600, "Invalid JSON-RPC request"));
    }
    let method = message["method"].as_str().unwrap_or_default();
    if id.is_none() {
        if method == "notifications/initialized" && *phase == 1 {
            *phase = 2;
        }
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Some(error(id, -32602, "Parameters must be an object"));
    }
    let result = match method {
        "initialize" => {
            if *phase != 0 {
                return Some(error(id, -32600, "Already initialized"));
            }
            let Some(version) = params.get("protocolVersion").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Missing protocolVersion"));
            };
            let supported = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
            let version = if supported.contains(&version) {
                version
            } else {
                "2025-06-18"
            };
            *phase = 1;
            json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":"maris","version":env!("CARGO_PKG_VERSION")},
                "instructions":"Read status and revision first. Validate proposals, then apply with expected_revision. Audio routing is user-controlled. Never infer headphone frequency response or acoustic SPL from digital telemetry."})
        }
        "ping" => json!({}),
        _ if *phase != 2 => return Some(error(id, -32002, "Initialize the MCP session first")),
        "tools/list" => tools(allow_write),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Missing tool name"));
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match invoke(store, name, args, allow_write) {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                }
                Err(e) => {
                    json!({"content":[{"type":"text","text":format!("{e:#}")}],"isError":true})
                }
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

pub fn serve_io(
    store: &Store,
    allow_write: bool,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<()> {
    let mut phase = 0;
    loop {
        let mut line = String::new();
        let length = Read::take(&mut *reader, MAX_JSON_BYTES + 1)
            .read_line(&mut line)
            .context("Read MCP input")?;
        if length == 0 {
            return Ok(());
        }
        if length as u64 > MAX_JSON_BYTES {
            serde_json::to_writer(
                &mut *writer,
                &error(Value::Null, -32600, "Request exceeds 64 KiB"),
            )?;
            writeln!(writer)?;
            writer.flush()?;
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(store, message, allow_write, &mut phase),
            Err(_) => Some(error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut *writer, &reply)?;
            writeln!(writer)?;
            writer.flush()?;
        }
    }
}
pub fn serve(store: &Store, allow_write: bool) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve_io(store, allow_write, &mut stdin.lock(), &mut stdout.lock())
}
