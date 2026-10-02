# Maris architecture

Maris separates audio rendering, device information, listening preferences and user interfaces. The terminal, menu bar, CLI and MCP use the same validated settings. This page describes the current source layout and the responsibilities of each module; [audio tests](audio-quality.md) record verification and [release status](../en/status.md) lists unfinished work.

## Source layout {#source-layout}

Only `main.rs` and `lib.rs` live directly under `src/`. The executable entry point calls `cli::main`; the library declares the modules below.

```text
src/
  main.rs
  lib.rs
  analysis/          Signal measurements, spectrum and music-context workers
  audio/             Stream ownership, platform capture, playback and recovery
    mix/             Multi-input and dual-output stream assembly
    pulse/           Linux local-server routing and PCM transport
  cli/               Command parsing, dispatch and command-specific adapters
  control/           Cooperative commands, profile storage and MCP
  devices/           Device identity, capability limits and AutoEq
  dsp/               Filters, music processing, limiting and resampling
  i18n/              Language selection, messages and translated labels
  mixer/             Mixer configuration, scenes and matrix processing
  models/            Local speech backend and reviewed model acquisition
  presets/           Built-in EQ catalog and listening scenes
  tuning/            Listening preferences and tuning proposals
  ui/
    desktop/         Menu bar, process lifecycle and compact monitor
    tui/             Terminal event loop, input and views
      settings/      Configuration drafts, rendering and confirmation
      studio/        Main dashboard layout and control geometry
    theme.rs
```

Use domain paths such as `crate::dsp::profile::Profile` and `crate::devices::identity::Identity` in production code. `lib.rs` retains the older public paths as re-exports so existing library callers continue to compile. A re-export names the same type or function; it does not create another implementation. Integration tests exercise both forms.

Do not add a generic `utils/` directory for code that belongs to a specific feature. Put a new source file next to its callers and declare it through its owning Rust module. Source files stay below 1,000 lines. Standalone tests remain under `tests/`; private unit tests are included from there with `#[cfg(test)]`.

## Module responsibilities

| Module | Responsibility |
| --- | --- |
| `audio` | Open and close streams, own callbacks and queues, handle session commands, select outputs and restore temporary routes. |
| `dsp` | Compile validated profiles and process samples. `music` contains correction and listening effects; `tone` contains filter coefficients; `resample` contains the clock bridge. |
| `analysis` | Measure signal statistics and loudness outside callbacks. `spectrum` produces display data; `context` schedules background music-context work. |
| `devices` | Keep stable device identities, capability limits, measurement matches and correction provenance separate from display names. |
| `tuning` | Persist listening preferences and create/apply bounded proposals. `eq` retains the existing ten-band planner; `planner` handles device-aware music preferences. |
| `mixer` | Validate and persist strips, buses and scenes. `engine` compiles fixed-size gain, pan, send and ducking coefficients. Hardware stream ownership belongs to `audio/mix`. |
| `models` | Run local speech and MusicNN adapters, report which models are available and consume build-verified embedded resources. |
| `presets` | Preserve Maris preset IDs, imported eqMac source curves and separate listening scenes. |
| `control` | Store the original EQ profile, serialize cooperative session commands and expose permission-checked MCP operations. |
| `cli` | Parse command-line arguments and call the corresponding product operation. It does not contain another audio processor. |
| `ui` | Render terminal and desktop controls, collect input and present operation results. |
| `i18n` | Translate presentation text while preserving command names, JSON fields, device names and identifiers. |

## Audio processing

The normal macOS system path is:

```text
System playback or selected applications
  -> private CoreAudio process tap
  -> bounded capture queue and clock bridge
  -> pre-EQ analysis feed
  -> shared EQ and stereo controls
  -> device correction and listening effects
  -> linked sample-peak limiter
  -> selected physical output
```

`audio/tap_ffi` and `audio/tap` own native process-tap calls and borrowed callback buffers. Global capture excludes Maris itself. Selected capture includes only the requested audio process objects. The tap mutes the original playback only while it is being used; releasing the tap restores normal playback. The normal path needs no BlackHole driver and does not change the system default output or volume.

`audio/system` assembles the macOS route and follows output changes. A disappearing pinned output falls back to the current system default. Each new output uses its own saved correction and preferences. Startup preserves correction, begins with neutral subjective controls and fades toward the saved target. The envelope waits for capture readiness rather than advancing during empty output callbacks. Both the temporary registration stream and final output must run an actual callback before capture starts; readiness is reset between those streams. The old, explicitly selected virtual-device route remains isolated in `audio/routing` and `audio/macos`; it is not the normal startup path.

On Linux, `audio/pulse` connects to the current user's local PulseAudio-compatible Unix socket, including PipeWire's pulse service. It creates owned temporary sinks, records original application routes before moving them and carries PCM through local capture/playback workers. Per-application mixer strips use independent private routes. Recovery checks server, process and stream identity before restoring routes, preserves manual changes and refuses flat-volume outputs. A separate helper watches for parent exit. Clients exposed only through native PipeWire and not through the Pulse service remain outside this path.

On Windows, `audio/windows` uses WASAPI process loopback for system or selected-process capture and CPAL for the physical output. `audio/windows_route` snapshots the affected render sessions, applies reversible session mute changes after capture startup, and attempts to restore the original mute state on drop. It does not change the endpoint master volume or system default device. Process-tree capture and application mixer strips use the same bounded DSP/output pipeline. `audio/windows_devices` binds selectors to WASAPI endpoint IDs and rejects changing enumeration snapshots rather than treating a display name as permanent identity. `audio/windows_mix` retains partially consumed blocks, sanitizes each source independently and fails explicitly when a selected source stops delivering normalized frames. `audio/windows_scope` checks a single process snapshot for self-exclusion and overlapping parent/child selections before capture. Mixer preflight checks all application strips together, not only each strip in isolation. Native readiness, mute/capture behavior, clock alignment and restoration still require Windows acceptance tests.

Windows output construction shares one pipeline builder for initial playback and switches. A candidate has its own analysis, revision and error telemetry, starts quarantined, and must execute a real silent output callback before the previous stream fades and retires. While quarantined it cannot dequeue shared capture PCM or publish applied settings. A failed start retains the previous stream; no successful `play()` request alone is treated as native readiness. Capture metrics remain separate from replaceable output metrics. The control thread polls stable default/pinned endpoint identity and native format, handles a vanished pin by selecting the unique physical default, and reports failed automatic switches instead of claiming application. Portable handoff tests exercise synchronous/asynchronous failures and retirement ordering; they do not execute WASAPI.

`audio/worker` owns each Windows worker before its startup handshake. Initialization errors, disconnected handshakes and timeouts cancel and join the worker; resources created by late initialization are released rather than becoming detached capture or routing threads. Route resources are owned before the mute operation and COM resources are dropped in their worker apartment. A platform API already blocked in the OS still has to return before joining completes; the handshake timeout is not a hard shutdown bound.

Explicit device input, file playback and the developing multi-device mixer use CPAL streams. Successful compilation of a platform backend is not a native runtime or device test.

### Multi-device mixer

`audio/mix` owns multiple explicitly selected inputs and up to two output streams. Its render module consumes each input once, applies per-strip EQ/compression, mixes the buses and sends secondary output through a separate bounded queue. Each output retains device-specific correction and final limiting. Native application sources can supply independent strips on macOS, Linux and Windows. A strip may explicitly run the existing speech-only RNNoise worker before its mixer DSP; normal music strips do not. Each output bus can add a fixed 0–500 ms alignment delay allocated before playback.

The primary output drives mixing. Input and secondary-output clock bridges account for their sample rates and independent clocks; filter tables and buffers are prepared before playback. `dsp/resample::ClockBridge::with_rates` supports the validated 44.1–192 kHz range and reduces the filter passband when downsampling. The original same-rate bridge entry point remains available.

Live coefficient updates are fixed-size values applied at block boundaries. Input or output assignment changes require an explicit stop/restart instead of silently opening another source from a settings write. The mixer requires capture authorization. Native multi-device operation, long-running drift, disconnect behavior and end-to-end latency still need acceptance testing; these are not established by generated PCM tests.

### Stream discontinuities and configuration changes

`audio/bridge::RenderState` treats missing source frames separately from valid digital silence. After an established stream loses readiness, it fades the last bounded output toward zero over 25 ms; resumed PCM crossfades from the current tail instead of jumping between unrelated samples. Filter history is reset without reallocating or releasing limiter attenuation. Uninterrupted music is not passed through a transient suppressor. Mixer strips share the recovery path while preserving their existing startup response and independent bus behavior.

The clock bridge has a bounded, allocation-free reset for observed capture overflow, timestamp discontinuity, output-callback gaps and unchecked configuration changes. A late native output callback restarts from silence and rejects old live backlog; the threshold scales with the actual previous callback size. Blocking local-server workers do not use this native-callback wall-clock heuristic. Rate-specific sinc tables are built once per bridge rather than constructed and immediately replaced.

On macOS, `audio/tap` records valid sample-time continuity and owns property listeners for tap format, aggregate rate/buffer configuration and available output rate/buffer/latency/safety/liveness properties. Listeners only publish an atomic generation. Unchecked generations quarantine capture and output; the control thread either acknowledges that exact unchanged configuration or rebuilds the native system pipeline. A mixer application source whose format changed requires an explicit mixer restart. Optional properties absent from a driver are not treated as required. Listener removal precedes freeing callback state. Raw callback pointers are obtained from shared references, not temporary exclusive references while listeners may be active. Startup rechecks configuration after listener registration before capture begins.

Runtime `continuity` telemetry identifies `stream-recovery-1` and reports observed capture/callback discontinuities, resets and configuration generations for the current pipeline. These are audio-path observations, not detected power-plug events or calibrated electrical measurements. A power-connect/disconnect noise report still requires device-specific physical acceptance; synthetic gap tests do not diagnose a charger or prove that the reported noise is eliminated.

### Callback restrictions

Callbacks do not allocate, take application locks, read files, use the network, write logs or invoke models. Control threads validate complete profiles, compile coefficients and enqueue bounded updates. Sample processing uses preallocated filter, delay and envelope state. Blocking Linux transport, Windows process-loopback capture and RNNoise inference stay on worker threads, not native audio callbacks.

The processor retains combined headroom calculation, bounded dynamic EQ, center-preserving stereo width, optional compression, Bass Assist and loudness-matched comparison. Correction and preference filters share the safety calculation. The final limiter bounds sample peaks; it is not a true-peak limiter or acoustic hearing protection. LUFS and true-peak measurements belong to analysis, not output normalization.

## Device settings and persistence

`devices/identity` maps stable platform identities to human-readable profile keys. Distinct devices must not share preferences merely because their display names match. When no stable identifier is available, the display-name fallback is explicit. `devices/capability` supplies permitted preference limits. `devices/autoeq` matches against the pinned catalog shipped with Maris; release builds embed the corresponding verified profile pack, while confirmed local profiles remain valid overrides. Exact unique matches and confirmed manual matches remain distinct from ambiguous candidates.

The main persisted files retain their existing names and schemas:

| File | Owner and purpose |
| --- | --- |
| `profile.json` | `control/store`: original EQ profile, revision and undo state. |
| `listening.json` | `tuning/preferences`: default and per-device listening settings. |
| `listening-previous.json` | `tuning/preferences`: previous listening settings for scoped undo. |
| `device-bindings.json` | `devices/identity`: stable-device bindings and profile keys. |
| `device-capabilities.json` | `devices/capability`: device limits and correction-match records. |
| `mixer.json` | `mixer`: strip/bus settings, revision, undo and saved scenes. |
| `runtime.json` | Running audio session: measured status and applied revisions. |
| `control.json` | Session-bound cooperative stop or routing request. |
| `pulse-route-<token>.json` | Linux temporary-route recovery record for one owned route group. |

Storage uses validation, locking and atomic replacement outside callbacks. `control::take_command` consumes requests under the producer lock for both the regular player and mixer. With no pending file, the reader returns without creating a lock file. Invalid JSON, oversized content and missing session identifiers produce an error and are removed so later requests can proceed. Directories and links are rejected without opening them; read errors and requests for another session are not silently consumed. Unchanged edits do not consume a revision or replace useful undo state. EQ and listening settings have separate revisions. A saved revision is not the same as a revision consumed by the running processor; the UI checks the applied counters.

Tonal presets and tuning proposals preserve measured correction and its provenance. AutoEq does not infer a headphone response from the music spectrum, and a matching profile does not prove that the physical unit, pads, fit or ANC mode match the measurement. Imported eqMac curves and converted DSP parameters remain separate; their fixed source and notices are in [third-party components](third-party.md).

## Terminal and desktop interfaces

During an active mixer session, the Apps page shows mixer channels rather than a separate application-capture selection. Left/right changes the selected channel's gain in 0.5 dB steps; Space toggles mute, X toggles solo, brackets adjust balance, and U uses mixer undo. Each edit checks the displayed session, source identity and mixer revision. Removed channels do not transfer their selection to the next row. Pending input/output assignments disable channel edits until an explicit mixer restart. The header compares saved and processor-applied mixer revisions as well as EQ and listening revisions.

Named mixer-scene saves and restores honor the CLI's expected revision. Saving a changed scene advances the mixer revision but preserves the preceding audio undo. Saving the identical scene again is a no-op. A scene update cannot silently race a restore because it participates in the same revision check.

`ui/tui` owns the event loop and terminal restoration. `ui/tui/studio` owns the main dashboard geometry; its `controls` module shares the rendered hit regions with input handling. `input` holds navigation and picker state, while `view`, `inspector`, `music`, `presets` and `monitor` render the corresponding surfaces. Daily listening stays on the dashboard; detail views remain optional.

`ui/tui/settings` separates the draft reducer, rendering and pointer confirmation. Browsing selects without mutating settings. Plus/minus actions edit the draft; Apply commits one validated revision and Cancel writes nothing. A complete press/release must target the same draft and geometry. Device, capability or revision changes invalidate the draft. A draft retains its target identity but checks freshness against current telemetry, not its original heartbeat. A deliberate adjustment separates successive Enter confirmations without permitting held Enter to replay into a new modal. Increment/decrement respects the current device's effective boost limit; a capped increment is a no-op, and decreasing a legacy over-limit value starts from the audible limit. Editing one parameter does not normalize unrelated saved preferences; device constraints are applied to the rendered profile. Opening an output or preset picker never applies its selection by itself.

`audio/session_control::queue_settings` is the saved-settings polling path called by the running Session. It also checks the device limits separately from EQ and listening revisions. A changed device limit recompiles the output even when the saved preferences are unchanged. The queue acknowledges revisions and limits only after enqueue succeeds, so a full queue retries on a later tick. Runtime `settings_pending` keeps both terminal and menu status from reporting a queued device-limit update as applied. Tests drive all 26 settings rows through the editor, persistence, this production polling path and the actual renderer; representative tone changes additionally compare output samples. The Unix pseudo-terminal test drives the real TUI process with delayed Enter and rapid consecutive edit/commit sequences. These are isolated, generated-signal tests, not physical-device or listening acceptance.

`ui/desktop/mod.rs` owns process lifecycle. `native.rs` builds menus and handles the native event loop; `controls.rs` handles validated quick controls; `events.rs` presents confirmations and launches the console; `monitor.rs` contains the macOS compact monitor. Native events wake the UI loop. Menu actions use the same scene, preference and session-command operations as the terminal. The menu and its tests share output-item validation for `uid:`, `pulse:` and `wasapi:` selectors. The displayed device and revision are checked before creating a preset or output preview, not only when applying it.

`i18n` provides six locales. Display text names the action or condition in ordinary language. The English display map, like translated resources, is separate from stable internal message keys. Neither changes JSON, proposals, placeholders, raw operating-system diagnostics or user-supplied names. Tests render every locale and check literal device-name preservation. Missing or stale measurements show unavailable state rather than animated substitutes. The visual design and interaction details are maintained in [interface design](../development/ui-design.md).

## Analysis, models and agent controls

`analysis` measures spectrum, RMS, crest factor, stereo correlation, LUFS and true peak on a worker. Level statistics retain their two-second cadence. A separate bounded collector supplies three real seconds of audio and measurements from that same window to `analysis/context`; incomplete inputs are not padded into recognition. Observed queue loss clears the partial recognition window and skips the pre-gap queue before collecting again. The context consumer rejects mismatched frame counts and lossy observations. Resets prevent a result from an old output from becoming the current context.

Builds explicitly requesting `MARIS_BUNDLE_SMALL_MODELS=1` embed the pinned MusicNN MTT weights after exact size and SHA-256 verification; `--release` alone does not request this resource. Actual inference and weight-digest tests are compiled only for verified bundled builds. Unbundled builds instead test explicit unavailability, not a silently skipped inference body. Malformed model probabilities invalidate the entire result rather than being filtered or clamped into plausible semantic evidence. `models/musicnn_preprocess` implements the fixed 16 kHz / 96-mel input path and `models/musicnn` runs the reviewed architecture through Candle on the background context worker. Recognition results still have confidence and age limits; unavailable, stale or failed inference falls back to signal analysis. Model-loading failures also start the signal-only worker, and the initialization error remains visible after subsequent processing or reset. The fixed FFT plan, Hann window and mel filters are shared immutably; each preprocessing call owns its scratch buffer. RNNoise remains an explicit speech-only path in `models` and `audio/voice`, not the default music processor. See [model review](../development/models.md) for provenance and remaining release evidence.

`tuning/planner` combines valid signal evidence, available semantic context, device limits, correction and current preferences. Preview does not write settings. Apply verifies the proposal and the current session/device/revisions, then uses the ordinary preference transaction. It does not edit correction. `control/mcp` starts read-only and requires write authorization for mutations. No agent operation runs arbitrary commands or uploads raw system audio.

## Build, documentation and verification

`build.rs` validates the vendored eqMac and AutoEq source snapshots, assembles the pinned AutoEq profile pack for release builds, and acquires the fixed MusicNN artifact only when a release build explicitly requests bundled resources. Both generated resources are integrity-checked and embedded before compilation finishes; normal playback does not fetch them. `scripts/` contains development and distribution tooling, not playback code. Normal installation downloads a platform-specific GitHub CI package; explicit local/developer builds are separate. Source checks, application builds, Pages deployment and public application release remain separate workflows. [Installation](../en/install.md) owns user instructions; [build and release](../development/releasing.md) owns packaging checks.

The website is generated from `docs/` Markdown and locale resources. UI illustrations come from actual offline Ratatui buffers. Moving a source file requires updating its include paths and illustration source bindings, then regenerating and checking the output. It does not justify reusing a stale verification report.

`tests/tooling/test_source_layout.py` checks root entry points, module ownership, domain imports, source-size limits and documentation agreement. `tests/module_paths.rs` checks old/new API identity and execution through the domain paths. Existing DSP, settings, menu, localization, mixer and sample-rate regressions remain part of verification.

Remaining release evidence is listed in the [product checklist](../development/product-gates.md) and [requirements file](../development/requirements.json). Numeric regressions, native-service runs, physical-device checks, hotplug/Bluetooth behavior and subjective listening results are recorded separately. Features outside the current product scope are not implied by this architecture document.
