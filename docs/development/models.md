# Included models and remaining checks

Reviewed: 2026-10-01. This is an engineering decision record, not a legal clearance or a listening-test certificate.

## Product decision

The app lists only implemented local processing. Models under consideration do not appear as available features or download buttons. A downloaded model still needs working code and tests. Recognizing a genre does not establish that the resulting sound is better.

| Component | Decision | What it does | Checks still needed |
| --- | --- | --- | --- |
| Native signal analysis and bounded DSP | Keep as the normal music processor | Implemented in Maris; no weight download or third-party inference runtime. Correction, preference, dynamic control, headroom and A/B remain separate. | Device-specific listening trials and long-running hardware regression. |
| RNNoise via pinned nnnoiseless 0.5.2 | Retain the existing built-in, explicit speech-only feature | The dependency embeds its default network. The upstream repository identifies BSD-3-Clause and documents embedded/custom models. Never enable it for music by default. | Distribution notice audit, real speech/noise listening evaluation and device cost measurements. |
| MusicNN MTT, pinned safetensors artifact | Bundle the small tagging model in release builds and run it only on the background context worker | The build pins revision and SHA-256; the Rust/Candle adapter implements the reviewed architecture and 16 kHz / 96-mel preprocessing. Its genre and instrument results can inform tuning suggestions; it never creates the output audio. | Final weight-redistribution notice review, native release-build execution on each architecture, CPU measurement and listening ablation against the signal-only baseline. |
| CLAP and the large genre classifiers previously listed | Remove from the executable's product registry | There is no working Maris integration or demonstrated listening benefit. | Reconsider only against a concrete customer problem and measured budget. |
| YAMNet / generic scene classifiers | Archive, not a music enhancement feature | Scene labels do not implement headphone correction or improve a waveform. | A specific validated use case, not additional labels. |
| DeepFilterNet | Do not add a second speech runtime to the music product | Upstream targets full-band speech enhancement. It is not evidence of universal music improvement; code licenses do not by themselves resolve every model asset's distribution terms. | A speech use case that demonstrably outperforms the already bundled path and justifies footprint/cost. |
| DeepAFx-ST | Do not copy, bundle, or use its implementation/checkpoints for commercial product development under the published license | The Adobe Research License restricts both use and redistribution to noncommercial research; section 1.3 explicitly excludes commercial product development. Published architectural ideas may inform independent design, but this is not permission to copy the implementation. | Separate appropriate permission before any commercial use of the licensed materials. |

## Primary sources checked

- https://github.com/jneem/nnnoiseless — README, built-in network and model loading; COPYING.
- https://github.com/xiph/rnnoise — speech-enhancement purpose; current upstream is not assumed identical to the pinned Rust port.
- https://huggingface.co/oriyonay/musicnn-pytorch — task, parameter count, model-card metadata and custom-code example.
- https://github.com/jordipons/musicnn — original music-tagging project, ISC repository license.
- https://github.com/Rikorose/DeepFilterNet — speech task and code-license scope.
- https://github.com/adobe-research/DeepAFx-ST/blob/main/LICENSE — Adobe Research License, sections 1.1–1.3.

Some raw/pinned web endpoints were unavailable during this review. No missing license or checkpoint evidence has been inferred from that failure. Previously pinned MusicNN hashes remain integrity constraints, not new permission to redistribute or evidence of model quality.

## Observed offline cost, not quality evidence

The release-mode `engine_probe` was executed on the connected macOS aarch64 workspace on 2026-10-01. It used eight seconds of generated 48 kHz stereo tones/noise in 800 ten-millisecond blocks, with warm-up and finite-output checks. The actual native music DSP spent 46.38 ms processing that input (0.580% of its audio-duration budget; 0.0590 ms p95 block wall time). The already bundled RNNoise path spent 49.98 ms (0.625%; 0.0648 ms p95). The raw result is `.maris-review/engine-cost.json`.

These are one-run offline kernel timings, not process CPU, full audio-pipeline cost, end-to-end latency, intelligibility or preference scores. No capture, device rerouting, model download or new music neural runtime occurred. The result supports the feasibility of retaining the existing embedded speech backend; it does not make it a music enhancer.

## Tests must execute the model

The earlier checkpoint test returned immediately when weights were absent. That green test did not prove inference execution. The build now exposes a checked `maris_musicnn_bundled` configuration only after exact resource verification; bundled tests require the real payload and recompute its SHA-256, while unbundled tests explicitly verify that inference is unavailable. NaN, infinity and out-of-range probabilities now reject the entire semantic result. A pre-fix regression demonstrated that NaN previously became a valid-looking semantic result.

`--release` alone does not enable the resource; bundling requires `MARIS_BUNDLE_SMALL_MODELS=1`. After the ordinary regression tests, the native push-check jobs now request the pinned weights and run the model tests. A download, digest or inference failure fails that job; a passing unbundled build is not counted as checkpoint execution. This test uses generated audio, not user playback. Reference-output comparison and listening acceptance remain separate checks.

## Resampling regression, 2026-10-02

A generated 10 kHz tone exposed missing low-pass filtering before conversion to 16 kHz. The previous code retained almost its entire amplitude, allowing it to appear at the wrong frequency. The corrected worker-only filter narrows its passband and increases its support for higher input rates. Tests at 44.1, 48, 96, 192 and 384 kHz retain a 1 kHz tone while suppressing the 10 kHz tone. The public analysis helper now uses this same filter instead of a separate linear converter, preserving its existing empty-input and duration behavior.

These tests check the converted waveform. They do not establish parity with a reference preprocessing library, model accuracy, or a preferred listening result. The default-worker test now checks the configured backend rather than incorrectly requiring a signal-only result from a build that includes MusicNN.

## Initialization failures and repeated setup

A model-loading error no longer prevents the audio session from starting. The context worker continues with signal analysis and keeps the initialization error in its status, including after a reset. A regression injects a loading failure and checks the actual worker output; it does not substitute a fake successful inference.

The 512-point FFT plan, Hann window and 96 mel filters are now initialized once. Transform scratch is local to each call and reused across its frames, so concurrent analysis cannot share mutable buffers. Tests compare every output value with a test-only copy of the previous implementation for complete windows and concurrent calls. Short inputs are rejected separately instead of being filled with invented silence. The setup-ablation test reports both runtimes without a brittle speed threshold. A debug-build timing is not a release-speed or model-CPU result.

## Complete recognition windows, 2026-10-02

Level measurements still update after two seconds of audio. Recognition now collects its own three-second window and measures those same samples before passing them to the model worker. The old preprocessing path accepted even a single sample and left the remaining time frames at zero; a regression reproduced that behavior before the fix. It now requires at least 48,000 real samples at 16 kHz, while valid three-second digital silence remains acceptable.

When the analysis queue loses frames, the recognition collector discards its partial window and skips the frames already queued at the observed gap. It then collects a new complete window, without changing playback or the level-meter cadence. The model worker also rejects an observation whose frame count does not match its audio, or whose measurements report dropped frames. Tests cover four source sample rates, repeated losses, counter resets, paired statistics, and the real analysis worker. These are software-input checks, not model-accuracy or hardware approval.

The handoff also records the loss counter alongside each completed observation. Retrieval rejects a counter change, even when the analysis thread has not polled again. The model worker rejects future observation times before changing its submission history, so a bad timestamp cannot suppress subsequent valid audio. The regression tests cover loss after publication, counter resets, exact-once retrieval and recovery after a future timestamp. This does not change the saved profile or audio routing.

Recognition results retain the captured window's measured bass and treble energy, level, crest factor and stereo correlation. A backend may add classification tags but cannot replace those measurements or change the observation time. A regression deliberately returns incorrect measurements and checks that the worker preserves the captured values while retaining valid tags.

## Before adding another model

Require a named listening problem, a pinned artifact, reviewed code and weight permissions, reproducible preprocessing/reference outputs, bounded offline CPU/memory/latency measurements, failure isolation, an integration test on the actual backend, and level-matched blind comparison against the existing DSP-only baseline. Do not train a network to imitate existing heuristics and then market that as a demonstrated improvement. A small classifier may be worth researching, but fitting in the installer does not satisfy these checks.

The developer CLI shows model sources and hashes. Small approved model files are acquired only during the release build, verified against fixed digests and embedded in the application. Large future models must justify their footprint and may use a separate optional download path; no such model is required for the current music pipeline.
