# gemm-common 0.19.0 patch

Source and MIT license are retained from the published crate. `UPSTREAM.json`
records the upstream revision, original SIMD hash and exact maintained source hashes.

Four AArch64 half-precision inline assembly helpers lacked a local `fp16`
target-feature boundary. In a baseline AArch64 debug build they can be emitted
out of line, producing `instruction requires: fullfp16` even though their callers
already perform runtime feature detection. Each helper now declares
`#[target_feature(enable = "fp16")]`, matching the existing scalar helpers.

The runtime dispatch, arithmetic, dependencies, crate version, and baseline CPU
features are unchanged. No global `+fp16`, `target-cpu=native`, optimized-profile
workaround, or disabled neural feature is required.

Architecture implementations now live in `src/simd/`, complex microkernel macros
in `src/microkernel/`, and dispatch macros in `src/gemm/`. These module moves
keep every maintained Rust file below 1,000 lines and preserve implementation
bodies, public exports, feature gates and runtime dispatch. The FP16 helpers are
in `src/simd/aarch64.rs`.

The same upstream failure is tracked at
https://github.com/sarah-quinones/gemm/issues/31.
