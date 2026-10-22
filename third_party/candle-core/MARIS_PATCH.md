# Maris ZIP 9 compatibility backport

This directory is based on the published candle-core 0.11.0 crate, not Candle's unreleased main branch.

- Published crate SHA-256: 5ecb245093b0f791b89d3420c3df9c6d49c60ab63ba54db896bf8a3baf486706
- Release source: https://github.com/huggingface/candle/tree/31f35b147389700ed2a178ee66a91c3cc25cc80d
- Compatibility change reviewed from upstream PR https://github.com/huggingface/candle/pull/4041 at 70f72e9cfdbfc570846ca363ead2135394af3e9a (still unmerged when backported).
- Licenses: MIT OR Apache-2.0; both upstream license texts are retained.

The production changes are limited to the ZIP 9 fallible filename API in src/npy.rs and src/pickle.rs. The normalized manifest pins ZIP 9.0.3 and declares a standalone workspace so upstream development targets do not become members of the application's workspace. Other imported production source bytes are unchanged from the verified crate. Binary upstream test fixtures and the upstream Cargo.lock are not part of this dependency snapshot; all UTF-8 source, test, example, benchmark and documentation files are retained.

This removes the ZIP 8 dependency responsible for RUSTSEC-2026-0367, RUSTSEC-2026-0368 and RUSTSEC-2026-0369 without importing unrelated unreleased tensor/backend changes. Application regression tests cover UTF-8 and CP437 NPZ names, eager/lazy tensor values and malformed NPZ/PTH errors. Native CI, model execution and cargo audit remain required; this patch does not provide physical audio acceptance.

The source-layout gate retains the exact upstream structure of eleven oversized Rust files using explicit SHA-256 baselines in `tests/tooling/test_source_layout.py`. They are not a directory-wide exemption: changing any byte, removing a baseline file, or introducing another oversized file fails the gate. The two locally patched Rust files remain below the project line limit. The complete dependency snapshot remains included in credential and source-digest audits.
