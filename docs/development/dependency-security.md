# Dependency security

## Current iteration: October 22, 2026

A new complete-lockfile audit found nine vulnerability advisories, so the historical clean result below is not a current approval. Cargo has updated `rustls` 0.23.45 to 0.23.46, `rustls-webpki` 0.103.15 to 0.103.16, and `webpki-roots` 1.0.9 to 1.0.10. These are the patched ranges for RUSTSEC-2026-0346/0347/0348/0349, RUSTSEC-2026-0382 and RUSTSEC-2026-0381. No ignore rule was added. A subsequent `cargo audit --deny unsound` loaded 1,348 advisories and scanned 548 locked dependencies: the six TLS findings were absent, but the command still exited 1 with the three ZIP vulnerabilities and four maintenance warnings below.

The three ZIP advisories RUSTSEC-2026-0367/0368/0369 remain open. `cargo tree --locked --target all -i zip` identifies `candle-core 0.11.0` as the parent of `zip 8.6.0`; a normal `cargo update -p zip --precise 9.0.0` fails because Candle requires `^8.6.0`. A compatible reviewed upstream dependency change or a maintained patch is still required. Compilation, package-entry validation and lack of an observed exploit do not clear a complete-lockfile advisory. Keep the existing audit and publication gates failing rather than suppressing these findings. Maintenance warnings for `paste`, `zbus_names`, `zvariant` and `zvariant_derive` are tracked separately.

The UI test parser was upgraded from `vt100 0.15.2` to `0.18.1` after a real resize-through-wide-character/erase panic was reproduced. Test callers now use nonzero terminal dimensions; the same regression and existing pseudo-terminal tests pass with the new parser. Cargo also updates `vte` to 0.15.0 and resolves `unicode-width` to 0.2.2. This parser is a development dependency, not an audio backend upgrade.

CPAL 0.17.3 was evaluated but is not installed: resolving it conflicts with the `alsa-sys` version introduced through `flexaudio-mic` and CPAL 0.16.0. The attempted update left the lockfile unchanged. CPAL 0.18 introduces additional error/rate behavior changes and needs a separate reviewed migration, not a blind version substitution.

Primary references: [Rustls secret-redaction advisory](https://rustsec.org/advisories/RUSTSEC-2026-0346), [certificate validation](https://rustsec.org/advisories/RUSTSEC-2026-0382), [trust roots](https://rustsec.org/advisories/RUSTSEC-2026-0381), [ZIP permission/symlink advisory](https://rustsec.org/advisories/RUSTSEC-2026-0369), [VT100 resize defect](https://github.com/doy/vt100-rust/issues/28), and [CPAL release notes](https://github.com/RustAudio/cpal/releases).

### ZIP 9 candidate investigation

Upstream [Candle PR #4041](https://github.com/huggingface/candle/pull/4041) remains unmerged at inspection. Its exact head `70f72e9cfdbfc570846ca363ead2135394af3e9a` changes ZIP filename reads to the fallible `Cow<str>` API and targets ZIP 9.0.3. [Issue #4042](https://github.com/huggingface/candle/issues/4042) separately requests a release. These upstream reports are not local acceptance evidence.

An isolated `.maris-review/candle-zip9-probe` declares that exact revision and CPU tests for NPZ names, malformed archives and compatibility with the registry NN crate. Its Cargo test command was rejected before execution because the protected-path scan exceeded 100,000 entries. No candidate tests ran, the main manifest and lockfile were not changed, and no alternate execution path was used. The three ZIP findings therefore remain open; adopting a Git patch also requires provenance, licensing and exact-source native regression review.

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
