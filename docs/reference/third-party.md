# Model provenance and third-party components

Initial research checked on 2026-09-30; model-admission and redistribution provenance is recorded here and in the pinned `third_party/*/SOURCE.md` and license files. This is an engineering provenance and notice record, not a legal clearance opinion. Native CLI/TUI packaging derives the locked runtime dependency inventory and verifies the actual archived notice bytes before a stable manifest can be assembled.

## Packaging and documentation tooling

The installer invokes system Bash and Apple bundle/signature tools and does not redistribute drivers or download models. Documentation, source audit, package assembly and round-report tooling use Python's standard library without bundled third-party Python packages. GitHub Actions references are pinned to exact commits in separate check/build/Pages workflows; their action code is not embedded in the Maris application.

Local package assembly includes this provenance record and the existing complete eqMac source/license notices. It deliberately records that the full dependency/model-asset notice audit is not complete. A public release must carry all notices required by every selected binary dependency and the project's chosen distribution terms; the current package manifest is not a substitute for that audit or a legal clearance opinion.

## Integrated neural backend

### RNNoise through nnnoiseless 0.5.2

- Upstream: https://github.com/jneem/nnnoiseless
- API: https://docs.rs/nnnoiseless/latest/nnnoiseless/struct.DenoiseState.html
- License source: https://github.com/jneem/nnnoiseless/blob/main/COPYING
- Maris pins `nnnoiseless` to 0.5.2 and uses its bundled default model, with no custom weight download.
- The package provides a Rust implementation of RNNoise-style recurrent speech denoising. Its declared license is BSD-3-Clause; the upstream COPYING file contains the authors' copyright notices, redistribution conditions, disclaimer, and non-endorsement condition.
- The low-level adapter requires 48 kHz frames and floating-point values scaled to signed 16-bit PCM. Maris scales at this boundary and discards/compensates the initial output frame for offline processing.
- The model is enabled only through explicit speech-enhancement actions. It can suppress musical content and is not used as a music-quality or headphone-correction predictor.
- Unit tests establish execution, finite/bounded output, sample-rate rejection, output frame count, and no-clobber behavior. They do not establish perceptual improvement, speech recognition accuracy, or hardware latency.

## Integrated preset source

### eqMac open-source advanced equalizer presets

- Upstream: https://github.com/bitgapp/eqMac
- Fixed commit: `04e5a3a9bd3a65f2b5105cf54a76d2c72a1d00d7`
- Preset source: `native/app/Source/Audio/Effects/Equalizers/Advanced/AdvancedEqualizerDefaultPresets.swift`
- Equalizer semantics source: `native/app/Source/Audio/Effects/Equalizers/Advanced/Equalizer.swift`
- Upstream license at the fixed commit: Apache-2.0.
- Maris vendors unmodified snapshots of those two Swift files plus the upstream LICENSE under `third_party/eqmac/`. The snapshots retain the upstream copyright notices. `SOURCE.md` records the commit and blob SHAs.
- `build.rs` extracts all 22 ten-band source presets from the fixed snapshot at build time. Stable Maris IDs use the `eqmac:` prefix; original names and raw gains remain available in the preset catalog.
- eqMac uses AVAudioUnitEQ parametric bands with 0.5-octave bandwidth and 0 dB global gain. Maris converts octave bandwidth to the current RBJ-Q representation at the negotiated sample rate and computes a separate conservative cascade headroom value with a 0.5 dB margin.
- Source gain values are not silently clamped. The source `Acoustic` preset retains its +18.22 dB band. Maris extends the profile gain range to accommodate the public source data and applies calculated digital headroom plus the final sample-peak limiter.
- The source curve and Maris conversion are not claimed to be bit-for-bit identical to AVAudioUnitEQ, and the safety calculation is not a true-peak or acoustic-SPL guarantee.

## Integrated signal-analysis dependency

### ebur128 0.1.10

- Upstream: https://github.com/sdroege/ebur128
- License: MIT.
- Maris uses it only on the analysis worker/offline analysis path for EBU R128 momentary, short-term and integrated loudness plus true-peak measurement.
- The real-time audio callback does not call the loudness analyzer. The final Maris limiter remains a sample-peak limiter; analysis true peak is telemetry/evidence and is not represented as a true-peak limiting guarantee.

## Bundled model and retained research

Normal UI/CLI/MCP inventory contains only local backends. MusicNN is bundled in release builds for background music tagging; the other investigations below are provenance notes, not selectable or downloadable product features.

### MusicNN PyTorch

- Source: https://huggingface.co/oriyonay/musicnn-pytorch
- Repository/model-card license metadata: Apache-2.0. The original `jordipons/musicnn` code is ISC-licensed. The pinned distribution record retains the Apache-2.0 text, the original ISC notice, source revisions and accepted weight digest under `third_party/musicnn/`; the native CLI/TUI notice inventory includes and hash-checks those files. This records the engineering redistribution basis and required notices without claiming legal clearance.
- Reviewed PyTorch-port revision: `7cff1a4f9899825ddba77130899dfac4c8cfe9d5`. The current MTT safetensors artifact is approximately 792k parameters / 3.18 MB F32 with SHA-256 `cc0b9400fcaed6e9ce7fbcfa97ec91e4fcb5f2ab34ca3a0cd6bef4af74753e1a`.
- The original MusicNN preprocessing downsamples to 16 kHz mono, uses a 512-point Hann STFT with 256-sample hop, 96 mel bands, then `log10(10000*x + 1)`. Upstream documentation recommends approximately three-second input patches because the models were trained with three-second inputs.
- The Hugging Face convenience path uses custom code (`trust_remote_code=True`). Maris does not execute that code and does not bundle PyTorch. Release builds obtain only the exact reviewed `model.safetensors` artifact from revision `7cff1a4f9899825ddba77130899dfac4c8cfe9d5`, require the expected 3,175,212-byte size and SHA-256 `cc0b9400fcaed6e9ce7fbcfa97ec91e4fcb5f2ab34ca3a0cd6bef4af74753e1a`, then embed the verified bytes. The native Rust/Candle adapter implements the reviewed MusicNN MTT architecture and runs only on the background music-context worker. Normal users do not fetch or import this model at runtime. The original ISC architecture snapshots used for comparison and provenance are retained under `third_party/musicnn/`.

### LAION CLAP Music

- Source: https://huggingface.co/laion/larger_clap_music_and_speech
- Model metadata license: Apache-2.0.
- Archived investigation, removed from the product registry. No Maris integration or incremental listening benefit has been established; it is not a current selectable feature.

### DeepAFx / DeepAFx-ST

- Upstream: https://github.com/adobe-research/DeepAFx and https://github.com/adobe-research/DeepAFx-ST
- Research reference for the architecture where a neural encoder predicts interpretable conventional audio-effect parameters and deterministic DSP performs rendering.
- No DeepAFx code, models or datasets are bundled in Maris.
- The DeepAFx-ST LICENSE at https://github.com/adobe-research/DeepAFx-ST/blob/main/LICENSE is the Adobe Research License. Sections 1.1/1.2 limit use and redistribution to noncommercial research; section 1.3 explicitly excludes development of commercial products. Do not copy or use these licensed materials for commercial Maris development without separate appropriate permission. This review does not assume another repository or checkpoint has the same license.

## Researched, not integrated

### DeepFilterNet

- Upstream: https://github.com/Rikorose/DeepFilterNet
- Purpose: low-complexity full-band speech enhancement at 48 kHz; upstream includes Rust processing and pretrained-model paths.
- Upstream states that repository code is dual-licensed MIT or Apache-2.0. Review the exact chosen model artifact, its notice requirements, version, and redistribution terms before integrating weights.
- Maris does not include its code or weights. The unused registry entry has been removed. Speech specialization alone does not justify adding a second runtime to the music product.

### YAMNet

- Primary documentation: https://www.tensorflow.org/hub/tutorials/yamnet
- Purpose: audio-event classification using a MobileNet-v1 architecture, with 521 AudioSet classes and 16 kHz mono input in the documented interface.
- Classification is not waveform enhancement, equalization, or a learned guarantee of preferred sound.
- Maris does not bundle a runtime or weights. Any conversion, runtime dependency, model version, license, and latency need separate review before integration.

## Signal-processing and platform references

- W3C Audio EQ Cookbook: https://www.w3.org/TR/audio-eq-cookbook/
- CPAL: https://github.com/RustAudio/cpal
- Ratatui: https://ratatui.rs/
- tray-icon: https://docs.rs/tray-icon/latest/tray_icon/
- BlackHole: https://github.com/ExistentialAudio/BlackHole
- VB-CABLE: https://vb-audio.com/Cable/
- Microsoft loopback recording: https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording
- MCP stdio transport: https://modelcontextprotocol.io/specification/2025-06-18/basic/transports
- Codex MCP configuration: https://developers.openai.com/codex/mcp/
- Claude Code MCP configuration: https://code.claude.com/docs/en/mcp

BlackHole and VB-CABLE are separately installed third-party virtual devices. Maris does not redistribute their drivers or imply affiliation. eqMac source snapshots used for the built-in preset catalog are redistributed only under the recorded Apache-2.0 terms and attribution above; no private or Pro preset data is included.

Maris's tonal planner is original bounded heuristic code, not a pretrained neural model. Its algorithm identifier and limitations are exposed with every generated proposal. Local model execution does not imply that external agent clients keep numeric summaries on-device; their own provider settings control that behavior.

## flexaudio-core security patch

Maris vendors the unmodified flexaudio-core v0.2.0 source under `third_party/flexaudio-core` and retains its MIT license. The local Cargo manifest changes only its ringbuf dependency from the upstream 0.4 range to ringbuf 0.5.2 so the Windows loopback path does not ship the memory-safety issue fixed by RUSTSEC-2026-0293. The vendored source remains attributable to Studio Sadola; Maris-specific rationale is recorded in `MARIS_PATCH.md`.

## gemm-common AArch64 FP16 patch

`third_party/gemm-common` retains the MIT-licensed published `0.19.0` crate from
upstream revision `86102c5b712737978371ac9ef7a11982f686d7bc`. The four vector
half-precision assembly helpers now declare their required `fp16` target feature,
matching the existing scalar helpers. This fixes baseline AArch64 debug
compilation; it leaves runtime CPU detection and non-FP16 fallback paths intact.
It does not raise the application-wide CPU requirement or disable model support.

`UPSTREAM.json` records the original and patched SIMD file hashes, and
`PATCHES.md` describes the four attribute additions. All other vendored crate
source and its license are unchanged. The source publication audit covers this
package with the same credential and path checks as other approved vendors.

The regression reproducer is a library depending on `gemm-f16 = "=0.19.0"`,
built with Rust 1.94.0 for `aarch64-unknown-linux-gnu` in the default debug
profile. The published dependency fails with eleven `fullfp16` assembler
errors; selecting this patch compiles without target-feature overrides.
The native Linux ARM build workflow additionally compiles and tests Maris itself.

## CLI/TUI archive notice inventory

`scripts/dependency_notices.py` selects the final native normal-dependency graph with locked Cargo metadata and `cargo tree --edges normal --target TARGET`. Every selected package contributes its declared license and actual shipped license/copyright/AUTHORS/NOTICE texts. When a published crate omits shared repository licenses, `third_party/rust-notices/index.json` binds texts fetched from its recorded `.cargo_vcs_info.json` source revision. The generated package index includes file hashes and source-archive pointers; locally patched crates also point to the exact Maris source commit.

The selected Rust compiler's original `COPYRIGHT-library.html` and `licenses/` files are also copied from its actual sysroot, with the compiler release, commit and host recorded separately from Cargo dependencies. Both staged-package and final-archive validation check every retained standard-library notice against its actual file hash.

The published `audio-core 0.2.1` crate declares `MIT OR Apache-2.0` but omits the repository-level license files from its crate archive. Its notice fallback is pinned to the exact VCS revision recorded by the crate and retains both upstream license texts. The published `dispatch 0.2.0` and `realfft 3.5.0` sources provide MIT and author declarations but no standalone license/copyright file. Their notice records preserve that omission and the published declarations, with the standard MIT permission and disclaimer. No missing copyright year or original notice is invented. This inventory does not claim legal clearance.

CLI archives also retain the original eqMac, AutoEq, MusicNN and flexaudio-core license/provenance records. MusicNN's port declares Apache-2.0 and the converted original checkpoint retains its ISC notice. A full Apache-2.0 text is included beside the original ISC record. The archive records the project's own license as declared or unspecified, matching the source; it adds no new open-source grant when none exists.

The CLI release collector checks these actual archived bytes against the native package's notice index and software evidence. GUI publisher signatures and physical-device/listening acceptance are separate, accurately reported boundaries.