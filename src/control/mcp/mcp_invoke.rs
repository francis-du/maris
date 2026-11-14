use super::*;

pub fn invoke(store: &Store, name: &str, arguments: Value, allow_write: bool) -> Result<Value> {
    match name {
        "maris_scenes" => {
            ensure!(
                arguments.as_object().is_some_and(|a| a.is_empty()),
                "This tool accepts no arguments"
            );
            Ok(
                json!({"scenes":crate::presets::scenes::SCENES,"count":crate::presets::scenes::SCENES.len()}),
            )
        }
        "maris_scene_preview" | "maris_scene_apply" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct SceneArgs {
                name: String,
                device: String,
                expected_listening_revision: Option<u64>,
            }
            let args: SceneArgs = serde_json::from_value(arguments)?;
            // The CLI/TUI transaction accepts the listening presets as well as
            // the scenes, so gate on the same resolver instead of the scene-only
            // catalog; otherwise MCP callers cannot reach presets the CLI accepts.
            ensure!(
                crate::dsp::music::MusicProfile::preset(
                    args.name.strip_prefix("scene:").unwrap_or(&args.name)
                )
                .is_ok(),
                "Unknown listening preset or scene"
            );
            let device = (args.device != "default").then_some(args.device.as_str());
            if name == "maris_scene_apply" {
                ensure!(allow_write, "Read-only MCP server");
                let revision = args
                    .expected_listening_revision
                    .context("expected_listening_revision is required")?;
                return Ok(serde_json::to_value(crate::tuning::preferences::preset(
                    store,
                    Some(revision),
                    device,
                    &args.name,
                )?)?);
            }
            ensure!(
                args.expected_listening_revision.is_none(),
                "Preview accepts no revision argument"
            );
            crate::cli::sound::run(
                store,
                Some(args.device),
                None,
                crate::cli::sound::Action::Preview { name: args.name },
            )
        }
        "maris_listening" | "maris_music_schema" => {
            ensure!(
                arguments.as_object().is_some_and(|a| a.is_empty()),
                "This tool accepts no arguments"
            );
            if name == "maris_music_schema" {
                return Ok(serde_json::to_value(
                    schemars::generate::SchemaSettings::draft07()
                        .into_generator()
                        .into_root_schema_for::<crate::dsp::music::MusicProfile>(),
                )?);
            }
            let runtime = audio::runtime_status(store);
            Ok(
                json!({"listening":crate::tuning::preferences::load(store)?,"output":runtime["output"],"applied_music_revision":runtime["applied_music_revision"],"engine_supports_music":runtime["music_processing"]==true}),
            )
        }
        "maris_music_apply" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct MusicApply {
                profile: crate::dsp::music::MusicProfile,
                device: String,
                expected_listening_revision: u64,
            }
            let a: MusicApply = serde_json::from_value(arguments)?;
            let device = (a.device != "default").then_some(a.device.as_str());
            Ok(serde_json::to_value(crate::tuning::preferences::edit(
                store,
                Some(a.expected_listening_revision),
                device,
                |p| {
                    *p = a.profile;
                    Ok(())
                },
            )?)?)
        }
        "maris_tuning_apply" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct TuningApply {
                proposal: music_tuning::Proposal,
            }
            let args: TuningApply = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(music_tuning::apply(
                store,
                &args.proposal,
            )?)?)
        }
        "maris_listening_undo" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ListeningUndo {
                expected_listening_revision: u64,
            }
            let args: ListeningUndo = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(crate::tuning::preferences::undo(
                store,
                Some(args.expected_listening_revision),
            )?)?)
        }
        "maris_autoeq_bind" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct AutoEqBind {
                device: String,
                model: String,
            }
            let args: AutoEqBind = serde_json::from_value(arguments)?;
            let entries = crate::devices::autoeq::load_cached(store)?;
            let entry = entries
                .iter()
                .find(|entry| entry.name == args.model || entry.path == args.model)
                .context("Model is not present in the cached pinned AutoEq index")?;
            crate::devices::autoeq::load_cached_profile(store, entry)
                .context("The selected AutoEq profile is not cached yet")?;
            Ok(serde_json::to_value(crate::devices::autoeq::bind_entry(
                store,
                &args.device,
                entry,
            )?)?)
        }
        "maris_device_profile_bind" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct DeviceProfileBind {
                stable_id: String,
                profile_key: String,
                expected_device_binding_revision: u64,
            }
            let args: DeviceProfileBind = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(
                crate::devices::identity::bind_profile_key(
                    store,
                    Some(args.expected_device_binding_revision),
                    &args.stable_id,
                    &args.profile_key,
                )?,
            )?)
        }
        "maris_output_select" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct OutputSelect {
                output: Option<String>,
                confirm_routing: bool,
            }
            crate::control::parse_output_selection(&arguments)?;
            let args: OutputSelect = serde_json::from_value(arguments)?;
            ensure!(args.confirm_routing, "Routing confirmation is required");
            let before = audio::runtime_status(store);
            crate::control::request_output(store, args.output.as_deref())?;
            Ok(json!({
                "queued":true,
                "action":"select_output",
                "session_id":before["session_id"],
                "output":args.output,
                "changes_system_default_output":false
            }))
        }
        "maris_application_scope" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ApplicationScope {
                pids: Vec<i32>,
                confirm_routing: bool,
            }
            let args: ApplicationScope = serde_json::from_value(arguments)?;
            ensure!(args.confirm_routing, "Routing confirmation is required");
            let before = audio::runtime_status(store);
            crate::control::request_applications(store, &args.pids)?;
            Ok(json!({
                "queued":true,
                "action":"select_applications",
                "session_id":before["session_id"],
                "pids":args.pids,
                "empty_scope_means":"system_playback_excluding_maris",
                "changes_system_default_output":false,
                "changes_system_volume":false
            }))
        }
        "maris_mixer" | "maris_mixer_capabilities" => {
            ensure!(
                arguments.as_object().is_some_and(|a| a.is_empty()),
                "This tool accepts no arguments"
            );
            if name == "maris_mixer_capabilities" {
                Ok(crate::mixer::capabilities())
            } else {
                Ok(
                    json!({"mixer":crate::mixer::load(store)?,"capabilities":crate::mixer::capabilities()}),
                )
            }
        }
        "maris_mixer_apply" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct MixerApply {
                config: crate::mixer::MixerConfig,
                expected_mixer_revision: u64,
            }
            let args: MixerApply = serde_json::from_value(arguments)?;
            args.config.validate()?;
            Ok(serde_json::to_value(crate::mixer::edit(
                store,
                Some(args.expected_mixer_revision),
                |config| {
                    *config = args.config;
                    Ok(())
                },
            )?)?)
        }
        "maris_mixer_undo" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct MixerUndo {
                expected_mixer_revision: u64,
            }
            let args: MixerUndo = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(crate::mixer::undo(
                store,
                Some(args.expected_mixer_revision),
            )?)?)
        }
        "maris_models"
        | "maris_platform"
        | "maris_analyze"
        | "maris_music_context"
        | "maris_device"
        | "maris_device_bindings"
        | "maris_applications"
        | "maris_performance" => {
            ensure!(
                arguments.as_object().is_some_and(|a| a.is_empty()),
                "This tool accepts no arguments"
            );
            if name == "maris_models" {
                return neural::models_at(store);
            }
            if name == "maris_platform" {
                return Ok(serde_json::to_value(crate::audio::platform::capabilities())?);
            }
            let state = audio::runtime_status(store);
            if name == "maris_device_bindings" {
                return Ok(serde_json::to_value(crate::devices::identity::load(
                    store,
                )?)?);
            }
            if name == "maris_applications" {
                return audio::applications();
            }
            if name == "maris_music_context" {
                ensure!(state["active"] == true, "No active audio session");
                return Ok(json!({
                    "context":state["music_context"],
                    "status":state["music_context_status"]
                }));
            }
            if name == "maris_device" {
                let output = state["output"]
                    .as_str()
                    .context("No active output device")?;
                let profile_key = state["profile_key"].as_str().unwrap_or(output);
                return Ok(json!({
                    "output":output,
                    "profile_key":profile_key,
                    "identity":state["device_identity"],
                    "binding_source":state["profile_binding_source"],
                    "binding_revision":state["device_binding_revision"],
                    "reported_output_buffer_frames":state["reported_output_buffer_frames"],
                    "reported_output_latency_frames":state["reported_output_latency_frames"],
                    "reported_output_safety_offset_frames":state["reported_output_safety_offset_frames"],
                    "reported_output_path_latency_ms":state["reported_output_path_latency_ms"],
                    "device":crate::devices::capability::inspect(store, profile_key)?
                }));
            }
            if name == "maris_performance" {
                return Ok(json!({
                    "performance":state["performance"],
                    "underruns":state["underruns"],
                    "overruns":state["overruns"],
                    "captured_frames":state["captured_frames"],
                    "reported_output_buffer_frames":state["reported_output_buffer_frames"],
                    "reported_output_latency_frames":state["reported_output_latency_frames"],
                    "reported_output_safety_offset_frames":state["reported_output_safety_offset_frames"],
                    "reported_output_path_latency_ms":state["reported_output_path_latency_ms"]
                }));
            }
            ensure!(state["active"] == true, "No active audio session");
            let analysis: crate::analysis::Analysis =
                serde_json::from_value(state["analysis"].clone())
                    .context("Analysis is warming up")?;
            analysis.validate()?;
            ensure!(
                crate::analysis::now_ms().saturating_sub(analysis.updated_at_ms) < 6000,
                "Analysis is stale"
            );
            Ok(serde_json::to_value(analysis)?)
        }
        "maris_tuning_suggest" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Goal {
                goal: String,
            }
            let args: Goal = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(music_tuning::from_live(
                store, &args.goal,
            )?)?)
        }
        "maris_autoeq_match" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Device {
                device: String,
            }
            let args: Device = serde_json::from_value(arguments)?;
            let entries = crate::devices::autoeq::load_cached(store)?;
            Ok(serde_json::to_value(crate::devices::autoeq::match_device(
                &args.device,
                &entries,
            ))?)
        }
        "maris_suggest" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Goal {
                goal: String,
            }
            let args: Goal = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(smart::from_live(store, &args.goal)?)?)
        }
        "maris_apply_suggestion" => {
            ensure!(allow_write, "Read-only MCP server");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Plan {
                proposal: smart::Proposal,
            }
            let args: Plan = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(smart::apply(store, &args.proposal)?)?)
        }
        "maris_preset_info" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct PresetInfo {
                name: String,
            }
            let args: PresetInfo = serde_json::from_value(arguments)?;
            let runtime = audio::runtime_status(store);
            let rate = runtime["sample_rate"]
                .as_u64()
                .filter(|rate| (44100..=192000).contains(rate))
                .unwrap_or(48000) as u32;
            Ok(serde_json::to_value(presets::show(&args.name, rate)?)?)
        }
        "maris_status" | "maris_presets" | "maris_schema" => {
            ensure!(
                arguments.as_object().is_some_and(|a| a.is_empty()),
                "This tool accepts no arguments"
            );
            match name {
                "maris_status" => {
                    Ok(json!({"state":store.load()?,"runtime":audio::runtime_status(store)}))
                }
                "maris_presets" => {
                    let catalog = presets::list();
                    Ok(json!({"count":catalog.len(),"presets":catalog}))
                }
                _ => Ok(serde_json::to_value(
                    schemars::generate::SchemaSettings::draft07()
                        .into_generator()
                        .into_root_schema_for::<Profile>(),
                )?),
            }
        }
        "maris_validate" => {
            let proposal: Proposal = serde_json::from_value(arguments)?;
            proposal.profile.validate()?;
            Ok(
                json!({"valid":true,"effective_preamp_db":proposal.profile.effective_preamp_db(),"profile":proposal.profile}),
            )
        }
        "maris_apply" => {
            ensure!(
                allow_write,
                "Read-only MCP server; explicitly launch with --allow-write to permit tuning"
            );
            let a: Apply = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(store.edit(
                Some(a.expected_revision),
                |p| {
                    *p = a.profile;
                    Ok(())
                },
            )?)?)
        }
        "maris_preset" => {
            ensure!(allow_write, "Read-only MCP server");
            let a: Preset = serde_json::from_value(arguments)?;
            let runtime = audio::runtime_status(store);
            let rate = runtime["sample_rate"]
                .as_u64()
                .filter(|rate| (44100..=192000).contains(rate))
                .unwrap_or(48000) as u32;
            Ok(serde_json::to_value(store.edit(
                Some(a.expected_revision),
                |p| {
                    *p = presets::apply_tone_curve(p, &a.name, rate)?;
                    Ok(())
                },
            )?)?)
        }
        "maris_undo" => {
            ensure!(allow_write, "Read-only MCP server");
            let a: Revision = serde_json::from_value(arguments)?;
            Ok(serde_json::to_value(
                store.undo(Some(a.expected_revision))?,
            )?)
        }
        _ => bail!("Unknown tool '{name}'"),
    }
}
