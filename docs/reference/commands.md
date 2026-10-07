# Command reference

The CLI is the scriptable interface to the same saved settings and audio engine used by the TUI and native menu. Run `maris --help` for the current command list and `maris <command> --help` for arguments.

## Global options

| Option | Purpose |
| --- | --- |
| `--lang <CODE>` | UI language: `en`, `zh-CN`, `zh-TW`, `ja`, `de`, `es` |
| `--json` | Emit structured JSON, including errors |
| `--no-tray` | Do not automatically start the native menu/tray process |
| `--expected-revision <N>` | Apply a write only if saved state still has revision `N` |

## Public commands

| Command | Purpose |
| --- | --- |
| `app` | Open the normal Maris application/TUI entry point |
| `package` | Build a local macOS application bundle from this executable |
| `update` | Update an installed CLI release to latest stable or a pinned version |
| `language` | Show or change the interface language |
| `sound` | Read or change device-specific listening settings |
| `mixer` | Configure multi-input processing and two output buses |
| `devices` | List available audio devices and stable identifiers |
| `applications` | List applications with audio without rerouting them |
| `application` | Process explicitly selected application PIDs |
| `models` | Show local/bundled analysis model status and provenance |
| `analyze` | Analyze an audio file |
| `smart` | Create a bounded tuning proposal from live or file analysis |
| `smart-apply` | Apply a previously saved tuning proposal |
| `enhance` | Apply speech-only RNNoise processing to a file |
| `voice` | Run live speech processing |
| `start` | Start a detached live/system session through the desktop controller |
| `stop` | Request the running audio session to stop |
| `tray` | Run or ensure the native menu/tray controller |
| `doctor` | Inspect native capabilities, permissions and recovery state |
| `status` | Show saved settings and runtime telemetry |
| `presets` | List, inspect, apply or restore global EQ presets |
| `preset` | Compatibility alias for `presets apply` |
| `band` | Edit one global EQ band |
| `preamp` | Set the global preamp |
| `bypass` | Toggle global processing bypass while retaining safety gain |
| `crossfeed` | Set global headphone crossfeed |
| `width` | Set global stereo width |
| `undo` | Undo the most recent compatible global settings change |
| `schema` | Print the JSON schema for the global DSP profile |
| `apply` | Validate or apply a global DSP profile JSON file |
| `export` | Export the global DSP profile to a new JSON file |
| `render` | Render an audio file offline through the global DSP profile |
| `run` | Process a live input device |
| `play` | Play a file through Maris processing |
| `tui` | Open the terminal UI |
| `system` | Process system playback with explicit routing authorization |
| `restore` | Restore platform routing owned by a previous Maris session |
| `mcp` | Serve the local MCP protocol; writes are opt-in |
| `integration` | Print MCP client configuration |

## Updating

~~~sh
maris update
maris update --dry-run
maris update --version 0.2.2
~~~

`update` is available from installed CLI releases that contain the bundled verified updater. It uses the same release manifest, archive checksum, executable checksum, transactional replacement and backup logic as the public installer. A development build does not silently fetch or install anything.

## Device sound

~~~sh
maris sound status
maris sound capability
maris sound match
maris sound match --apply
maris sound bind "MODEL"
maris sound schema
maris sound scenes
maris sound preview dialogue
maris sound preset warm
maris sound set --bass 1.0 --presence 0.5 --adaptive 0.4
maris sound compare true
maris sound save-device
maris sound import ./correction.txt --source "measurement source"
maris sound apply ./profile.json --dry-run
~~~

Writes without `--device` require a current active output with a known profile key. Use `--device default` for default preferences or `--device "NAME"` / a stable device selector for an explicit profile.

## Global EQ and profiles

~~~sh
maris presets list
maris presets show eqmac:acoustic
maris presets apply warm
maris presets restore
maris band 1 -2.0 --frequency 60 --q 0.8
maris preamp -3
maris bypass true
maris crossfeed 0.2
maris width 1.1
maris undo
maris export ./profile.json
maris apply ./profile.json --dry-run
~~~

Global EQ and device-specific listening settings are deliberately separate revision/undo domains.

## Applications and system audio

~~~sh
maris applications
maris application --pid 1234 --accept-routing
maris system --accept-routing
maris system --output "Device name" --accept-routing
maris stop
maris restore
~~~

Commands that can temporarily reroute or replace playback require explicit authorization. Maris does not change system volume.

## Mixer

~~~sh
maris mixer status
maris mixer capabilities
maris mixer add music --device "Input"
maris mixer set music --gain -3 --send-a 1
maris mixer eq music warm
maris mixer band music 1 -2
maris mixer compressor music --enabled true --threshold -18 --ratio 2
maris mixer bus 0 --device "Output" --gain 0
maris mixer duck --enabled true --threshold -24 --attenuation 8
maris mixer scene-save desk
maris mixer scene-restore desk
maris mixer undo
maris mixer run --accept-routing
~~~

Mixer capture/routing is explicit and keeps its own revisioned configuration.

## Files and analysis

~~~sh
maris analyze ./input.wav
maris render ./input.wav ./output.wav
maris play ./input.wav
maris smart --file ./input.wav --goal balanced
maris smart --save ./proposal.json
maris smart-apply ./proposal.json
~~~

`enhance` and `voice` are speech-only RNNoise paths and require `--confirm-speech`; they are not general music enhancement commands.

## JSON and MCP

~~~sh
maris --json status
maris --json sound status
maris mcp
maris mcp --allow-write
maris integration codex
~~~

MCP starts read-only. `--allow-write` enables the same validated settings operations exposed by the CLI; it does not grant arbitrary shell execution or direct system-volume control.

## Safety and state

Mutating commands validate ranges and revisions before saving. Routing commands require explicit authorization. Preset/device changes are reversible through their compatible undo path. Runtime telemetry is not treated as proof of physical-device behavior; see [audio verification](audio-quality.md) for the test boundary.

## Development contract

Source builds require Rust 1.94. The current DSP behavior revision is `music-fidelity-14`; changing audible processing requires updating that revision and its regression evidence.
