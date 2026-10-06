# Command and protocol reference

The maintained English reference for Maris listening controls, state, CLI and MCP. For normal precompiled installation, use the dedicated installation guide rather than a source-build command.

The core goal is enjoyable music on the current output device. Device correction, personal taste, and speech cleanup are different tasks. Maris does not equate more effects, more bass, or higher volume with better sound. It does not infer a headphone's frequency response from the spectrum of a song.

## Install and documentation

The normal standalone Bash/PowerShell installer downloads precompiled native CI release assets. Users do not need a source checkout or Rust. Explicit `--from` / `-From` and developer `--build` / `-Build` are separate alternatives. Per-user validation, native identity/signature checks, backups and no-audio-activation behavior remain shared. Installation never starts the native macOS, Windows or Linux system-audio path.

See [installation](../en/install.md) for signed-install behavior, destination options, recovery and uninstall. The static documentation source is [documentation](../README.md); GitHub Pages is configured for `https://maris.francis.run/` after repository Pages enablement and a successful deployment. [build and release](../development/releasing.md) separates push/PR checks, manual application builds and documentation deployment. A configured workflow is not evidence of remote success.

Only source, tests, scripts, documentation and required third-party attribution belong in Git. Build artifacts, local runtime settings, model caches, `.wcode` state and verification logs remain ignored. `python3 scripts/source_audit.py --manifest` creates an audited staging list.

## Open and listen

On macOS 14.2 or later, open **Maris.app** and allow system-audio recording when macOS requests it. Maris automatically connects system playback to the current physical output, processes it live, and displays a menu-bar indicator. No BlackHole, manual routing, microphone selection, or audio-file import is required for normal music listening.

From a terminal, run `maris` to open the console with the same automatic connection. `maris tui` attaches to an existing session or starts system processing when stopped. Closing the console does not stop music; use **S**, `maris stop`, or the menu's **Stop processing**. **Quit Maris** also stops the owned session. Updating the executable does not replace an already-running process: quit the old application once and open the new build.

The native system tap excludes Maris's own playback and mutes the original application path only while the tap runs. It does not permanently replace the system's default output. Default-output changes trigger a rebuild of the native connection. Physical hotplug, Bluetooth, protected content, and every application/device combination have not all been validated.

## Music enhancement and device preferences

The default console is a single **Studio Dashboard**, designed around professional audio interfaces rather than equal-sized status cards. One large analyzer shows the real pre-EQ spectrum and a separately scaled calculated profile EQ; stereo output meters are docked at its right. A fixed rail contains device correction/capability facts and all ten primary listening controls. Music context, Listening Assist preview and mixer/health summaries remain on the same screen. E/M/V/I/H directly open the fixed-position EQ, Apps, Device, Listening Assist and Health inspectors; the same actions are clickable. Tab is optional, and Esc returns home. See [interface design](../development/ui-design.md) for the reference study, composition and visual-review criteria. These controls run in the real-time system playback chain, not only in a file renderer. Static tone controls are neutral by default; a mild adaptive EQ is enabled by default to make bounded cuts only when low-frequency buildup, upper-mid harshness, or high-frequency prominence dominates its band-limited detector. It never adds gain. Compression and RNNoise are not enabled by default.

Click a listening parameter to select it, then click its visible **− / +** controls or use Left/Right to adjust. Scrolling selects parameters without changing values. The header's output and preset areas open the existing pickers; Enter confirms. The quick rail remains usable in the read-only inspectors. The Sound settings editor instead presents a read-only parameter browser and a separate draft edit station; it has no live +/- targets inside list rows. Pointer actions do not pass through dialogs or act on controls hidden by compact mode. The header reports pending versus applied revisions rather than treating a saved setting as proof of audio application.

Press **B** when no configuration draft is pending, **Space** in Dashboard, or click the header's comparison control, to compare the enhancement with its reference. The comparison estimates rolling stereo mean-square power and attenuates the louder path. It never boosts either path for matching. This is not calibrated acoustic loudness or ITU loudness measurement. Reference A retains the original ten-band EQ, shared headroom and final limiter; it is a reference for the additional music stage, not a bit-perfect system bypass.

TUI listening edits are already persisted through the device-specific listening library; the separate save-device key was removed from the simplified console. On macOS, Maris binds preferences to CoreAudio device UID/model-UID evidence when available and keeps a human-readable profile key, so a renamed or reconnected endpoint can retain the same preference. The first device using a display name can reuse the historical name key; a different stable device with the same display name receives a distinct key instead of silently sharing correction. If a stable platform ID is unavailable, Maris falls back explicitly to the display name rather than inventing identity.

```sh
# Writes without --device require a fresh active output and a known profile key.
maris sound status --json
maris sound preset warm --json
maris sound set --bass 1.5 --presence 0.5 --softness 0.2 --adaptive 0.6 --virtual-surround 0.7 --json
maris sound compare true --json
maris sound compare false --json
maris sound save-device --json

# Explicit default or named-device preferences.
maris sound --device default preset natural
maris sound --device "USB Headphones" preset detail
maris sound schema
maris sound apply ./music-profile.json --dry-run --json
```

The original music presets `natural`, `warm`, `vocal`, `detail`, `soft`, and `night` remain available. Ten additional listening scenes provide practical starting points without a model download. They change device-specific preferences, not the original ten-band EQ or measured correction. All scenes preserve correction filters/provenance/preamp, correction high-pass, channel balance, processing enable, Reference A/B state and the user's comparison level-matching policy, then apply the current device capability limits. The TUI preset picker reopens on the last applied layer while it still matches the current state; if another surface changed that layer, it falls back to an exact current scene or EQ match rather than resetting to the first row.

| Scene ID | Intended use and parameter direction |
| --- | --- |
| `focus` | Subtle bass/treble reduction for background listening; no compression. |
| `long-listening` | Softer high end and bounded dynamic control; not hearing protection. |
| `dialogue` | Reduced bass and a small presence lift for dialogue/podcasts; no denoising. |
| `cinema` | Modest weight and dialogue presence; preserves any separately selected spatial effect. |
| `surround-360` | Virtual 360 side decorrelation above the bass region; preserves the current tonal/dynamics settings. |
| `cinema-360` | Gentler virtual surround for film playback; preserves the current tonal/dynamics settings. |
| `stereo-focus` | Reduces unstable Side energy while preserving the mono center and current tonal settings. |
| `night-dialogue` | Reduced bass and peak dynamics; explicitly enables compression without makeup gain. |
| `acoustic-listening` | Restrained presence/air lift for acoustic recordings; compression stays off. |
| `orchestral` | Neutral static tone and very light dynamic control, preserving original stereo width. |
| `tight-bass` | Bass reduction and stronger bounded dynamic control, not more sub-bass. |
| `game-clarity` | Less bass masking and modest presence; not positional or surround enhancement. |
| `small-speakers` | No sub-bass boost; mild virtual bass only for permitted speakers with a known low-frequency limit. |

The existing `night` preset and new `night-dialogue` scene enable compression explicitly. The other new scenes do not. These are subjective starting points, not an automatic recognition feature or evidence of improved sound on every device.

```sh
maris --json sound scenes
maris --json sound --device "Studio Headphones" preview night-dialogue
# Read the returned listening revision before an automated write.
maris --json --expected-revision 0 sound --device "Studio Headphones" preset night-dialogue
```

The preview reports final device-constrained parameters and changes without writing state or starting audio. Applying a scene uses the ordinary listening revision/undo transaction. It never changes the OS volume or output route.

Bass is bounded to -6..+6 dB; presence/air to -3..+3 dB; softness/intensity/adaptive strength to 0..1; width to 0..1.5. Adaptive EQ uses three internal relative-energy detectors around bass buildup, upper-mid harshness, and high-frequency prominence. At full strength its individual cuts are bounded to approximately 1.5, 1.5 and 2 dB; the default strength is 45%. Imported correction and positive boosts reserve digital headroom. Music changes use the existing full-chain crossfade and final sample-peak limiter.

### Built-in equalizer preset catalog

Maris keeps the original six ten-band profile IDs and adds 21 non-Flat presets in a separate `eqmac` category generated from the public eqMac source at fixed commit `04e5a3a9bd3a65f2b5105cf54a76d2c72a1d00d7`. The fixed source still contains 22 presets including `Flat`; source extraction tests verify all 22. Maris `flat` is now a true 0 dB unit-response profile, so the public eqMac Flat curve is deliberately deduplicated and `eqmac:flat` remains only a compatibility alias to `flat`.

```sh
maris --json presets list
maris --json presets show eqmac:acoustic
maris --json presets apply eqmac:bass-reducer
maris --json presets restore
```

The source frequencies are 32, 64, 125, 250, 500, 1000, 2000, 4000, 8000 and 16000 Hz. eqMac's 0.5-octave parametric bandwidth is converted to Maris's RBJ Q representation at the negotiated sample rate; it is not treated as `Q=0.5`. Source gains are preserved, including `Acoustic` at +18.22 dB. Maris calculates cascade headroom with a 0.5 dB safety margin and retains that headroom during eqMac bypass comparison so a high-gain preset does not jump back to 0 dB. The final limiter remains sample-peak protection, not a true-peak or hearing-safety guarantee.

Press **P** for one picker containing the thirteen listening scenes first, followed by the original 27 Maris/eqMac EQ curves. Rows distinguish **Scene** from **Tone curve**. The spatial scenes are **Surround 360**, **Cinema 360**, and **Stereo Focus**; they use the listening revision/undo domain and do not replace the global EQ curve. Up/Down selects without changing sound; the scene preview shows its purpose, preserved correction, compression warning and final device-limited changes. Enter applies once, Esc cancels, B compares and U restores the previous change. No additional tab or nested setup is required. The original `presets` CLI/MCP catalog remains 27 EQ curves for compatibility; `sound scenes` and the scene tools expose the separate listening catalog.

### External device correction

An optional importer reads a strict subset of Equalizer APO/AutoEq parametric text: `Preamp`, `PK`, `LS/LSC`, and `HS/HSC` with frequency, gain and Q. It accepts at most 16 enabled filters and rejects unknown enabled operations, duplicate indices, invalid numbers, and oversized inputs. A source label is retained with the correction. It does not execute include directives or download files.

```sh
maris sound --device "My Headphones" import ./measured-parametric.txt --source "Model / measurement provider / target / revision"
```

This is an optional advanced workflow, not a prerequisite for system playback. Imported parameters are user-supplied evidence, not measurements made by Maris. AutoEq discovery is pinned to commit `7ae0f56d53074872b028649617a22bbb4232feb7`: the recommended-model catalog is shipped with Maris, and release builds embed the matching parametric profile pack. Exact unique endpoint names may auto-match; fuzzy or variant matches require confirmation; generic names such as `Headphones` never silently select a model. A locally cached confirmed profile may override the bundled copy, but no runtime download is needed. The precise model, pads, fit, ANC mode, measurement source and target still need verification; Maris does not perform acoustic calibration.

## Compact monitor and console

The current palette is graphite with low-chroma accents. Small terminals automatically fall back to the compact console instead of adding a separate compact-mode key. Ordinary dialogs size themselves to their content. The macOS menu's **Compact monitor** toggles a draggable, non-activating native window with a 264 by 76 point content area plus native chrome. It shows real spectrum/level motion and the current adaptive reduction when active. Closing that window does not stop audio. Presets are grouped into a submenu.

A separate fast pre-EQ spectrum and output peak meters drive the animation. The UI interpolates at approximately 30 frames per second; the engine publishes telemetry approximately every 80 ms. Silent, stopped or stale data decay, rather than being replaced by decorative motion. New visualization fields require restarting an older running engine. The native compact window is currently macOS-only; compact TUI rendering is platform-independent.

## Menu Bar quick controls

The native Menu Bar now shows the actual output, the current device's listening preset and the separate global EQ curve. Preset names are matched against the actual device-constrained preference values; a manual edit becomes Custom rather than retaining an obsolete scene label. Applied requires both callback revision counters; a saved change remains Pending until the engine reports it.

The TUI preset picker exposes these as separate layers even when they share a display name. For example, **Warm · Listening preset** changes the current output's listening profile and is the same setting used by the Menu Bar, while **Warm · Tone curve** changes the global ten-band EQ. External Menu Bar/CLI/MCP edits move the TUI selection to the layer whose revision actually changed; neither layer silently overwrites the other.

Choose **Select output** or **Listening presets** directly in the menu. Output items retain stable device identity, not a mutable row number. Listening presets are grouped in one submenu, including the spatial-effect scenes. Selecting an enabled output or preset applies it immediately through the same revision/session/device guards used by the TUI and CLI; there is no second confirmation dialog. Applying a scene, switching output, comparing and undoing do not require opening the TUI or navigating a tab.

On macOS, Windows and Linux, choosing an enabled preset or output commits immediately after the OS menu event while still rechecking the displayed session, output identity, capability and revisions. No native review dialog or hidden Apply/Cancel step remains. Native events wake the main event loop through an event proxy rather than depending only on periodic polling. Only the controller's own successfully committed revisions are acknowledged immediately, so rapid A/B or Undo after a selection is not rejected as an outside edit; real concurrent edits still fail closed.

**Compare reference / enhanced** and **Undo listening change** are direct menu actions. A/B still uses the same revisioned parameter path but no longer overwrites the previous tonal change's undo snapshot. Current-device Undo cannot restore another device's settings. Six native language names are available directly in the menu and update the existing shared preference. Errors remain in a dedicated message rather than being overwritten by the next status refresh.

The existing global bypass also puts the music stage into its reference branch. Its menu label is therefore **Bypass processing (keep safety gain)**, not EQ-only bypass. While globally bypassed, listening preset application and A/B are unavailable until processing is enabled; the interface does not suggest that an inaudible enhancement has been heard. Digital safety gain and the final sample-peak limiter remain in place.

## Sound settings: browse, edit, apply

Press **E Settings** to open the Sound settings editor. Three fixed selectors in the left browser are **1 Device sound**, **2 Global EQ**, and **3 Playback switches**. Each is one click or key, not a Tab sequence. The right-hand edit station shows **Current** and **Draft** values. Selecting a visible row does not recenter the list or move another target under the pointer. Arrow and wheel navigation stay inside the selected section, so scrolling EQ cannot land on global bypass. Clicking a parameter or using any arrow/wheel only selects it. List rows have no live increment/decrement buttons. Use the separated **+ / −** controls to stage a value and **[ / ]** to stage band Q. Space does not toggle anything in this editor.

**Enter / Apply changes** commits the displayed draft once. A mouse Apply requires pressing and releasing inside the same button on the same draft; dragging, resizing, a keyboard input or a changed draft cancels that incomplete gesture. Toggle controls show explicit **OFF / ON** choices instead of ambiguous plus/minus values. Ordinary tone edits do not implicitly enable music processing or exit Reference A; playback policy is changed only through its own controls. The configuration header does not act as a live A/B button. **Esc / Cancel draft** drops it without touching saved settings or audio. Apply and Cancel occupy fixed, separated bottom targets, and the header distinguishes an unapplied draft from the running audio. Multiple changes in one scope commit as one ordinary undoable transaction. One draft cannot mix global EQ and device-listening transactions; apply or cancel before editing the other scope. Pending drafts block unrelated navigation, preset/output actions, A/B and Undo rather than silently discarding work. Stop remains available. The usual first-band path is **E → + → Enter**, with no extra tab or confirmation wizard.

The calculation preview is labeled as a draft, not applied audio or a headphone measurement. New external edits, stale telemetry, device/capability/rate/revision changes or the existing preview expiry invalidate it until cancellation. Shrinking into compact mode cannot apply hidden changes and retains an Esc-cancel instruction. Routing and measured correction are not editable from this preference/EQ draft. The default Studio dashboard retains its existing bounded live quick controls. Configuration changes remain local drafts regardless of those live controls; this difference is stated on the settings page. Playback switches have separate scope/effect warnings, including whether correction is disabled and whether all outputs are affected.

## Configuration safeguards

An output or preset picker retains the session, device identity/binding, sample rate and both configuration revisions that the user saw. Changing that context or leaving the selection pending for over a minute invalidates confirmation. An invalidated TUI picker stays modal until cancelled, so a queued Enter cannot act on the screen underneath. Delayed current-device parameter input is rejected if the output changed; unavailable current telemetry does not silently redirect interactive listening edits into a fallback profile. Explicit offline CLI configuration remains supported with `--device default` or a named device profile. A missing, stale, future-dated or unidentified current output cannot silently redirect an implicit CLI write into the fallback. `sound set` with no parameters is rejected; `sound --device default save-device` is rejected instead of saving the active device by accident.

Mouse motion is coalesced in bounded input batches while all key/button press, release, drag and resize events retain order. Physical mouse clicks clear the keyboard duplicate gate; deliberate rapid plus/minus clicks are not swallowed as held keyboard toggles. Discrete key repeat events and short duplicate-key bursts are filtered for confirmation, toggles and Undo; continuous arrows, plus/minus and Q adjustment remain available. This is not a claim that every terminal reports physical key releases identically. A/B preserves the tonal undo domain, and exact no-op profile/listening writes retain the last useful revision and undo snapshot. Turning an already-off effect down leaves it off; the first upward Bass Assist or Dynamic EQ step starts at 10%, not a hidden cached amount. Bass Assist increases are rejected when the selected device capability disallows them. A clamped/no-op control does not enable processing, leave Reference A, rename the EQ curve or redirect Undo to another library. Explicit SaveDevice can still create a named copy of fallback settings.

Current-device Undo restores only that endpoint. An A/B change on another endpoint does not invalidate the last tonal undo and is retained during undo/redo; a different endpoint's actual tonal edit still blocks that scoped undo.

A route request cannot silently overwrite another unconsumed request from a menu, TUI or agent. Duplicate requests are idempotent, different pending requests return an explicit busy error, and Stop retains priority. The producer and control-thread consumer share a short file transaction outside all audio callbacks. New route commands carry the observed output-rebind counter, which the engine checks before applying. An omitted or malformed `output` field is rejected: only explicit JSON null means follow the system default. Malformed session-bound action/guard fields are reported as failed controls, not interpreted as routes or allowed to stop a healthy stream.

## Languages and console

English, Simplified Chinese, Traditional Chinese, Japanese, German and Spanish resources now cover the Studio and inspectors, output/preset/help/start confirmations, known route/device/state values, scene names/descriptions, tuning deltas/reasons, notices, menu entries and tooltips. Press **L** to change the persisted language. Notices retain their template and arguments until rendering, so an existing notification can change language without changing its device name or numbers. Open consoles and the tray observe saved language changes; explicit startup `--lang` is preserved until the saved preference changes, and child consoles inherit the chosen language.

CLI commands, JSON fields, preset IDs, device names, model identifiers and standard audio units remain unchanged. Known product errors receive localized messages; unfamiliar OS/library diagnostics keep their original cause under a localized technical-details label rather than hiding useful evidence. Resource tests check all five translations, duplicate keys and named-placeholder parity. Render tests exercise all six languages in the home, every inspector, overlays and compact mode.

```sh
maris language zh-CN
maris --lang ja tui
maris language
```

| Control | Action |
| --- | --- |
| E / M / V / I / H | Directly open EQ / Apps / Device / Listening Assist / Health; Esc returns home |
| Tab / Shift+Tab | Optional inspector cycling, never required to reach a feature |
| Dashboard Up/Down, Left/Right | Choose and adjust any of the ten primary listening controls |
| Dashboard mouse | Click selects; selected-row −/+ adjusts; the wheel selects only; header output/preset opens pickers |
| Sound settings arrows / wheel | Browse grouped parameters only; no sound or saved state changes |
| Sound settings + / −, [ / ] | Stage the selected value or band Q in memory |
| Sound settings Enter / Esc | Apply one draft / cancel without writes; Esc with no draft returns home |
| Space in Dashboard | Toggle reference/enhanced A/B; Space does nothing in Sound settings |
| O | Open the output picker; Up/Down chooses; **Enter is required to switch**; first row follows system default |
| P | Open the flat preset picker; Enter applies; Esc cancels |
| Mixer Matrix Up/Down, Space, Enter | Choose apps, mark a pending scope, then apply it once |
| A in Mixer Matrix | Mark all system playback; Enter confirms the new scope |

When there is no local Apps draft, the TUI follows the active runtime capture scope, including changes made through CLI, MCP or the native menu. Local marks stay stable while editing. Leaving Apps before Enter discards those unapplied marks and restores the active runtime scope.

| Assist goal buttons | Click Balanced/Warm/Clear/Soft to generate a preview; no change is applied yet |
| G / J / Enter | Choose a listening goal / preview / explicitly apply the preview |
| B | Compare reference/enhanced through the same level-matching path |
| U / L / S / Q | Undo / language / stop audio / close only the console |

The Profile EQ curve displays only the original ten-band filter response, not a headphone measurement or the complete dynamic device-processing response. The measured pre-EQ spectrum shares its frequency axis but uses a separate dBFS scale. Both scales and the scope label remain visible. Output meters use actual sample-peak telemetry. Small terminals receive a reduced layout.

## Agents and intelligent control

CLI, TUI and MCP share atomic, revision-checked state. The original ten-band profile remains backward compatible. Additional music preferences live in `listening.json` with their own revision; no existing original profile is migrated or silently overwritten.

On macOS the same status also reports `coreaudio_tap_mode`: `device_stream` means Maris bound the process tap to one unambiguous mono/stereo output stream, while `global_mixdown` means CoreAudio's compatibility stereo-mixdown path is in use. Maris does not apply a guessed fixed gain correction to that fallback because multi-output/multichannel mixdown behavior is device/graph dependent.

`maris status --json` reports the original `applied_revision` and the additional `applied_music_revision`. `maris sound status --json` shows the effective device preference and whether the running engine implements the music stage. A newly saved preference is not proof that an old engine has applied it. `engine_revision`, `restart_required`, and `pending` distinguish the new client from a still-running old engine. This fidelity revision is `music-fidelity-12`: it retains the correction-first 120 ms physical-output startup/rebind ramp, linked Bass Assist stereo invariants and original-path Level Match, and moves Virtual 360 decorrelation above a 250 Hz Side high-pass so bass localization is better isolated from the spatial effect while mid/high Side decorrelation remains active. Conservative EQ/correction headroom therefore remains available before processing while ordinary program loudness can return close to the original path before the final linked limiter. Explicit user preamp attenuation remains deliberate, and turning Level Match off keeps the conservative headroom level. Older running engines are reported as restart-required rather than pretending to apply the new behavior. Historical untouched Maris built-in profiles that still carry the old fixed -6 dB preamp are compatibility-normalized to a 0 dB requested preamp when their built-in curve is unchanged; custom profiles keep their explicit preamp.

```sh
maris mcp                    # read-only
maris mcp --allow-write      # explicitly allow validated tuning
maris integration codex
maris integration claude
maris integration other
```

Additional read tools include `maris_scenes`, `maris_scene_preview`, `maris_listening`, `maris_music_schema`, `maris_music_context`, `maris_device`, `maris_device_bindings`, `maris_applications`, `maris_platform`, `maris_performance`, `maris_tuning_suggest`, and `maris_autoeq_match`. AutoEq matching uses the pinned catalog shipped with Maris; normal use does not refresh the catalog over the network.

Validated product write tools include revision-checked `maris_scene_apply`, `maris_music_apply`, `maris_tuning_apply`, monotonic `maris_listening_undo`, `maris_autoeq_bind`, revision-checked `maris_device_profile_bind`, and explicit live route tools. The model inventory reports only local backends. MusicNN semantic inference runs on the background context worker when the release build contains the pinned weights; there is no runtime model-fetch MCP tool. Context-aware proposals carry device identity, listening revision, signal evidence, semantic context, capability limits, before/after preference and limitations; apply rejects stale or modified proposals and never changes measurement correction. No MCP tool changes OS volume or executes arbitrary shell commands.

The built-in `smart` planner remains a local, bounded, signal-aware heuristic. It now receives EBU R128 momentary/short-term/integrated loudness and true-peak measurements in addition to RMS, crest factor, spectrum and stereo correlation; those measurements are evidence, not automatic loudness maximization. It is not a learned sound-quality model and does not automatically keep flattening the music spectrum. Human edits invalidate stale proposals. Music-stage preferences and original EQ proposals are separate controls.

## Speech and file tools

Normal music processing requires **no model download**. The existing native DSP and signal-analysis backend are the music foundation. The pinned RNNoise network is built into neural-enabled builds and remains explicitly speech-only; `maris voice` and `maris enhance` require `--confirm-speech` and 48 kHz input. Do not use speech suppression as a universal music enhancer.

The customer UI contains no model-download controls. Listening Assist presents useful goals, evidence readiness, processing state, comparison and undo. Release builds use bundled MusicNN tags when current and confident; builds without verified weights, inference failures and stale results fall back to measured signal features. `maris models` and MCP inventory report only local backends and never trigger a network fetch.

`maris models research` reports the pinned MusicNN provenance and whether the current build contains the verified weights. Release builds acquire the exact pinned artifact during the build, verify its size and SHA-256, and embed it for the native Rust/Candle inference adapter. Normal users do not download or import this small model. Other unused model candidates remain outside the executable registry. See [model review](../development/models.md) and [product gates](../development/product-gates.md).

`maris play`, `maris tui --play`, `maris render` and `maris analyze` remain optional PCM WAV tools. The basic offline renderer uses the original ten-band profile; it does not implicitly read a current device's additional listening profile. Normal application listening uses live system audio instead.

## Control panel, mixer state and devices

The Studio Dashboard is the default single-page listening surface. Fixed-position inspectors expose the full EQ controls, native application scope, device identity, Listening Assist and diagnostics without displacing primary listening controls. Sound settings opens on its remembered EQ band. Its parameter browser groups device preferences separately from global EQ and has no live mutation targets; a fixed edit station stages changes before explicit Apply. The old grid of equal framed cards and the four mandatory workspace tabs have been removed. `O` now opens an explicit output picker and has **no routing side effect until Enter**; the first item returns Maris to system-default following. This replaces the previous press-O-to-cycle behavior that could cause abrupt device/profile changes. If a pinned device disappears, the native backend releases the old path, falls back to the current system default and recompiles that device's saved listening profile/capability state. A new output starts from correction-preserving neutral preferences and crossfades into the saved subjective target over 120 ms. Mixer Matrix uses Space to mark a pending set and Enter to apply it once; A marks all system playback and still requires Enter. The persistent mixer state model still supports up to 16 logical strips, two buses, gain, mute, solo, pan, sends, scene save/restore and deterministic voice-trigger ducking math through the CLI/state layer.

```sh
maris --json mixer capabilities
maris --json mixer status
maris --json mixer add music --device "Input Device"
maris --json mixer set music --gain -3 --pan 0.2 --send-a 1 --send-b 0.25
maris --json mixer scene-save Listening
```

The mixer data plane opens multiple explicit hardware or application sources and up to two physical outputs. Each strip has independent gain, pan, mute, solo, sends, EQ and compression; neural-enabled builds also expose explicit speech-only RNNoise per strip. Input and secondary-output clock bridges track independent devices. Each output bus has gain, mute and a bounded 0–500 ms alignment delay. Assignment, denoise and delay changes require an explicit mixer restart rather than silently opening or rebuilding routes from a live settings write.

## Platform and advanced-feature status

The Rust DSP, state, CLI, TUI, localization and model adapters are cross-platform source. Native system/application paths are implemented with CoreAudio process taps on macOS 14.2+, WASAPI process loopback plus reversible session routing on Windows, and a local PulseAudio-compatible route (including PipeWire-Pulse) on Linux. `maris --json applications` is read-only; `maris application --pid <PID> --accept-routing` starts explicit selected-process processing. Implementation does not equal three-platform hardware validation; the native CI and device checks remain separate release evidence.

Each Maris stream is stereo. The multi-strip/two-bus engine supports simultaneous explicit inputs and two outputs, independent application strips on the native platform backends, per-strip processing, independent clock bridges and user-set output alignment delay. Per-device preference memory remains separate from mixer routing. Virtual 360 is a bounded stereo-side spatial effect, not multichannel object audio, head-tracked binaural rendering or a room model. Maris does not claim unrelated plug-in hosting or room-measurement features.

The clock bridge uses a preallocated 128-tap windowed-sinc interpolator for near-unity independent-clock correction with a 50 ms native capture reserve. This is not an arbitrary sample-rate converter or a measured end-to-end latency figure. The original EQ and music filters now share one estimated headroom reserve instead of independently attenuating the signal twice. Disabled music processing no longer applies its own gain. See [audio quality](audio-quality.md) for reproduced regressions, research, tests and limitations.

## Tuning reliability

Preview readiness rejects stale/future/incomplete evidence and sample-rate mismatches. Semantic hints require a real available backend and a current observation. Bass-heavy material receives a small reduction of positive bass preference rather than an accidental jump to zero; device limits remain authoritative. The planner does not silently enable another compressor on already low-crest material. Displayed changes are generated from the final constrained profile, including enable/disable transitions. Apply verifies both the profile and explanation, and a no-op does not consume an undo snapshot or advance revision.

## Build and verification

Use Rust 1.94 or newer. macOS needs Xcode command-line tools; Windows needs MSVC build tools; Linux needs ALSA development headers and `pkg-config`. Default features are `desktop` and `neural`.

```sh
cargo fmt --all --check
cargo test --all-targets --locked
cargo test --all-targets --no-default-features --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
cargo run --release --locked --example engine_probe  # offline cost only, not sound-quality evidence
cargo run --release --locked --example engine_probe -- --ablation  # conditioned numerical contribution, not listening preference
cargo run --locked --example menu_probe  # isolated native-menu construction/locale review; no capture
cargo run --locked --example ui_probe -- --settings --draft  # six-locale real draft/buffer review in temporary state
./target/release/maris package  # create dist/Maris.app on macOS
```

Tests cover filter response, finite limited output, neutral defaults, compression, estimated A/B energy matching, strict PEQ parsing, device isolation, revision conflicts, MCP permissions, localization completeness, compact console rendering and configuration misuse. Menu/controller tests exercise selection/cancel without writes, preview invalidation, UID-bound output selection, comparison-safe undo, no-op history, competing route writers and malformed control input. The native menu probe constructs actual items in all six locales, then dispatches their native IDs through the production handler with injected confirmation responses, recording `.maris-review/menu-review.json`. It never consumes the user's menu events or changes their settings, and does not claim a physical dialog click. The Unix pseudo-terminal regression drives the actual TUI process through motion bursts and plus/minus/Apply clicks with a parent-held audio lease and isolated state; it never starts capture. A native system-audio session was previously observed processing live audio with zero underruns/overruns after buffer adjustment. This is not a listening-quality certification or proof of every hardware path. Local application bundles are not Developer-ID signed or notarized.

References: [RBJ filter equations](https://www.w3.org/TR/audio-eq-cookbook/), [AutoEq](https://github.com/jaakkopasanen/AutoEq), [CamillaDSP](https://github.com/HEnquist/camilladsp), [EasyEffects](https://github.com/wwmm/easyeffects). These references support the specific design choices described above; Maris does not claim feature parity with those projects.
