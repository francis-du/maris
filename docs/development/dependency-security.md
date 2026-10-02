# Dependency security

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
