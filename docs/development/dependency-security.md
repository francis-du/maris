# Dependency security

## Current iteration: October 22, 2026

The complete-lockfile audit initially found nine vulnerability advisories. The security baseline delivered in [PR #84](https://github.com/francis-du/maris/pull/84) updates `rustls` to 0.23.46, `rustls-webpki` to 0.103.16, `webpki-roots` to 1.0.10, and ZIP to 9.0.3. It does not add advisory ignores or weaken `cargo audit --deny unsound`.

The exact reviewed head `1fa6300beaff2341d5c72a8123794971c24f3239` passed the complete audit and native macOS, Windows and Linux checks in [run 39827117582](https://github.com/francis-du/maris/actions/runs/39827117582). That audit loaded 1,349 advisories and scanned 550 locked dependencies; none of the nine TLS/ZIP vulnerability findings remained. Four maintenance warnings remain visible: `paste`, `zbus_names`, `zvariant` and `zvariant_derive`. A successful audit does not mean that these warnings disappeared. Main commit `a502d07578a02aa3d58ae94acd3b247fd03245f1` contains the same source tree as the reviewed test merge.

The UI parser integration must pass the same complete-lockfile audit and native checks against its final lockfile. The earlier baseline is evidence for the reviewed security change, not automatic approval of later dependency edits.

### ZIP 9 compatibility backport

Candle 0.11.0 requires ZIP 8 in its published manifest. The compatibility patch retains the published `candle-core 0.11.0` crate, whose SHA-256 is `5ecb245093b0f791b89d3420c3df9c6d49c60ab63ba54db896bf8a3baf486706`, and backports the ZIP 9 filename API adaptation from [upstream PR #4041](https://github.com/huggingface/candle/pull/4041) at `70f72e9cfdbfc570846ca363ead2135394af3e9a`. The upstream PR was unmerged when inspected; its unrelated unreleased changes were not imported.

Of the 65 production source files in the verified crate, 63 remain byte-for-byte unchanged. Only `npy.rs` and `pickle.rs` adapt ZIP's fallible `Cow<str>` filename API. The normalized manifest pins ZIP `=9.0.3`; the MIT and Apache-2.0 license texts and source provenance are retained in [the patch record](../../third_party/candle-core/MARIS_PATCH.md). Eleven oversized upstream files have exact SHA-256 baselines in the source-layout gate; modifying one, removing one, or introducing another oversized file still fails that gate.

The native checks execute UTF-8 and CP437 NPZ filename cases, eager/lazy tensor-value comparisons, malformed NPZ/PTH rejection, and the pinned bundled models. They also retain default/minimal regressions and strict Clippy. The initial local probe was rejected before execution and was not counted as test evidence. Physical audio and listening acceptance remain separate from these software and archive-compatibility checks.

### Terminal parser and audio backend review

The UI test parser was upgraded from `vt100 0.15.2` to `0.18.1` after a real resize-through-wide-character/erase panic was reproduced. Test callers use nonzero terminal dimensions. The same regression and existing pseudo-terminal tests passed on the local candidate. Cargo also updates `vte` to 0.15.0 and resolves `unicode-width` to 0.2.2. This parser is a development dependency; the combined integration still requires current native CI.

CPAL 0.17.3 was evaluated but is not installed: resolving it conflicts with the `alsa-sys` version introduced through `flexaudio-mic` and CPAL 0.16.0. The attempted update left the lockfile unchanged. CPAL 0.18 introduces additional error/rate behavior changes and needs a separate reviewed migration.

Primary references: [Rustls secret-redaction advisory](https://rustsec.org/advisories/RUSTSEC-2026-0346), [certificate validation](https://rustsec.org/advisories/RUSTSEC-2026-0382), [trust roots](https://rustsec.org/advisories/RUSTSEC-2026-0381), [ZIP permission/symlink advisory](https://rustsec.org/advisories/RUSTSEC-2026-0369), [VT100 resize defect](https://github.com/doy/vt100-rust/issues/28), and [CPAL release notes](https://github.com/RustAudio/cpal/releases).

## Historical native-event dependency replacement

Maris audits the complete `Cargo.lock`, including platform dependencies that do
not compile on the current host. Native reachability checks are an additional
boundary, rather than a substitute for that audit.

The former Tao dependency resolved GLib 0.18.5 through its GTK backend even
though Maris only selected Tao on macOS and Windows. That version is affected by
[RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): the
variant string iterator passed an immutable pointer to a mutating C output
argument. Avoiding the code on one host did not remove the affected package.

The native event receiver now uses Winit 0.30.13 with default features disabled,
only on macOS and Windows. Linux continues to use the existing StatusNotifier
tray. The generated lockfile contains neither Tao nor GLib/GTK packages. No
upstream source is vendored for this change and no advisory is ignored.

Both push/PR checks and the six-architecture artifact workflow install the
pinned `cargo-audit` 0.22.2 tool and execute:

```sh
cargo audit --deny unsound
```

The `unsound` denial is deliberate: the default audit reports this GLib advisory
as a warning and can otherwise exit successfully. Against the former lockfile,
the same command exits 1; against the replacement lockfile it exits 0. The
remaining `paste` maintenance warning is visible and is distinct from an
unfixed soundness advisory. Artifact builds depend on this audit job.

The event receiver must also remain functional:

```sh
cargo run --locked --example menu_probe
cargo run --locked --example menu_probe -- --event-loop
```

The first probe checks real native menu construction and validated controls in
six locales. The second runs the production application receiver against a
temporary state directory, changes the saved language through a native item
event, and requires a bounded shutdown with no active status item left behind.
macOS invokes the actual AppKit menu action; Windows sends the native item ID
through the event-loop proxy. Both probes avoid audio startup, capture and
routing. They do not certify physical devices or subjective sound quality.
