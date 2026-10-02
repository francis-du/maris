# Product checklist

Maris is still in development. This checklist describes the work required before a public release; it is not release approval.

## What the app should do

The user should be able to choose an output, adjust its sound, review a suggestion, compare the result at a similar volume, and undo a change. Normal music playback must work locally without installing a model or learning the internal audio architecture.

Keep common controls on the main screen. E/M/V/I/H open settings, apps, device details, suggestions and diagnostics. Labels should describe an action or a result. Internal names such as message keys, JSON fields and model identifiers are not display copy.

## Required release checks

- Test headphone and speaker connections, Bluetooth interruptions, missing devices and an unavailable default output on real hardware. Test output identity and recovery, not just device enumeration.
- Compare output switches, presets and A/B at similar listening levels. Digital peak limits and generated audio tests do not establish good sound or hearing protection.
- Keep the reported power-connect/disconnect noise issue open until it is checked on the affected setup. Compare the symptom with recorded audio-buffer and device-change counters. Do not ask for repeated loud exposure to reproduce it.
- Display the settings that will actually be used after device limits. Reject old, future-dated or incomplete analysis. No-change operations must preserve undo. Saved settings must not be reported as applied until the audio processor receives them.
- List only available, implemented models. Missing, failed, old or uncertain MusicNN results must fall back to audio measurements. New models need a stated purpose, reviewed code and weight permissions, reproducible inputs, and measured cost before inclusion.
- Run formatting, Clippy, default and minimal-feature tests, the recorded offline rounds, and an optimized build on the final source. Review real rendered screens in six languages, small-window layouts, clicks, keyboard controls and confirmations. Preserve literal device names, message arguments, correction settings and undo.
- Check installation, upgrades, failure recovery and final archives on each operating system. Finish third-party notices, the project-license decision, publisher verification and platform signing requirements. macOS signing, notarization and stapling are separate from a successful build. Keep signing credentials outside the repository.

## Build and publication

The source includes per-user installers for macOS, Linux and Windows, system-audio implementations, separate source-check/build/website workflows, package checksums, and a recorded offline-test runner. Each platform still needs its own observed build and runtime results. A workflow declaration, archive header or passing Mac test is not a Windows or Linux result.

A local package is a development build until the public-release checks pass. The release script requires the matching source, test report, final reviewed package and platform signature checks. It does not invent device or listening-test approval.

Pushing source to the owner's repository, producing test packages, publishing the website and distributing an application are separate actions. Permission to push source does not authorize a release or signing operation. All previously requested work remains listed in [requirements](requirements.json).

## Keep, combine or remove

| Area | Decision |
| --- | --- |
| EQ and headphone correction | Keep separate settings and sources. Similar filter code does not make a personal EQ setting the same as measured device correction. |
| Dynamic EQ, virtual bass, compression and stereo width | Keep separately testable controls. Compression and virtual bass remain opt-in. Test each effect with audio that should activate it and with audio that should not. |
| Digital headroom and final limiter | Keep active in every comparison. Do not remove clipping protection to improve a benchmark. |
| Models | MusicNN recognizes music; it does not produce the output waveform. RNNoise is speech-only. Do not add models simply to lengthen the feature list. |
| Main-screen controls and Settings | Keep immediate small adjustments separate from changes prepared for later application. Browsing must not save. Pointer motion must not discard button presses or releases. |
| Menu confirmation | Show the proposed preset or output change before applying it. Keep the pending-menu entry as a fallback, without requiring another menu search on supported native dialogs. |
| Shared event code | Use the same console-launch and confirmation handling for menu entry points instead of duplicating it. Test actual native item IDs. |
| Analysis resampling | Use one anti-alias filter for model input and the public analysis helper. Preserve the helper's duration and empty-input behavior. Test retained and rejected frequencies across input rates. |
| Repeated model-input setup | Reuse the fixed FFT plan, window and mel filters. Each worker keeps its own scratch buffer. Compare every output value with the previous implementation and test simultaneous calls; report timings without assuming a speedup. |
| Session commands | The regular player and mixer consume commands through one session-checked reader. An old mixer must never remove a newer session's command. |
| Mixer | Keep independent inputs and two outputs, with per-channel sound controls. Speech noise reduction must be selected explicitly. Use output delay for alignment, not as an undocumented sound effect. |

## Compare effects one at a time

```sh
cargo run --release --locked --example engine_probe -- --ablation
```

The probe writes `.maris-review/feature-ablation.json`. It runs three generated signals at four sample rates through the real processor, comparing an all-effects reference with eight versions that each omit one effect. It reports level-matched differences and processing time. The all-effects reference is not a default preset.

A difference shows what a component changes, not that listeners prefer it. An effect can correctly have no contribution to a particular input; stereo width on mono is one example. Keep the gain calculation and limiter in every variant.

## Input and failure tests

`tests/terminal_input.rs` launches the actual console in an owned pseudo-terminal with isolated settings and an audio-session lock that prevents capture. It checks event bursts, button edges, delayed Enter and exact-once application. `menu_probe` sends actual native item IDs through the production handler with a test confirmation response; it is not a human click test.

Fault-injection tests should fail with the old bug present, then pass after the fix. Include stale settings, device changes, full queues, invalid files, timeouts and failed starts. Record skipped, failed and interrupted cases as such. The 200-round runner combines seeded audio/settings tests with rotating regression tests; it is not 200 independent audits or 200 runs of the entire suite.

The 2026-10-02 regression cases additionally cover future or malformed runtime timestamps, old analysis windows arriving after newer ones, and high-frequency aliasing during model-input resampling. Each of these tests failed with the earlier behavior before the fix. Windows application changes now validate the complete selected process tree before stopping healthy capture; native Windows execution is still a separate check.

Additional regressions exercise the menu's actual output-item selection for macOS, Windows and Linux IDs; reject clicks from a menu drawn for an earlier device; and check mixer commands belonging to another session, malformed files and links. The model-start test injects a loading error and requires ordinary audio analysis to continue with the failure still visible after a reset. These tests failed with the earlier implementations before their fixes.

Wording checks cover the six displayed languages, shared startup messages, literal device names and each template's named arguments. Internal JSON fields and message keys stay stable. Documentation illustrations must be regenerated after changing display text.

Generated audio can establish repeatability, numeric limits and processing cost. Physical device behavior, end-to-end delay and listening preference require separate checks.
