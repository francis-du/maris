# Release status

Maris is still in development. The list below uses the same data as the release checks. Implementation, automated tests and real-device tests are recorded separately.

## Required work {#scope}

Open items cover features, installation, the interface, documentation and reported bugs. New problems must be checked too; deleting a list entry cannot make a release ready.

## Automated tests {#checks}

Tests cover audio processing, saved settings, keyboard and mouse actions, undo and installation. The 200-round check uses different generated inputs and rotates related tests. It is not 200 complete audits and cannot replace listening tests. Code changes require fresh results; skipped or interrupted tests are not passes.

## Devices and packages {#native}

macOS, Windows and Linux each need their own build and test results. Headphones, speakers, Bluetooth, device disconnects and power changes also need physical checks. Public packages require matching test results, device checks, license review and publisher verification. The normal installer does not accept development candidates.

## Publication {#publication}

Pushing source, building packages, updating the website and publishing an app are separate operations. A successful push does not mean the download is available or every test passed. See [build and release](../development/releasing.md) for the maintenance procedure.
