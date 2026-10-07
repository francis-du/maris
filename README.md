# Maris

System audio tuning for macOS, Windows and Linux.

Maris processes audio already playing on your computer. It provides device-specific listening settings, headphone correction, EQ, comparison/undo, application processing, and a multi-input/two-output mixer through the TUI, native tray/menu UI, CLI, and MCP.

## Install

Normal installation downloads the prebuilt release for the current platform and verifies it before replacing an existing installation. It does not install Rust, change the system volume/default output, install an audio driver, or modify PATH.

### macOS / Linux

~~~sh
curl -fsSL https://maris.francis.run/install.sh | bash
~~~

### Windows PowerShell

~~~powershell
irm https://maris.francis.run/install.ps1 | iex
~~~

For pinned versions, offline packages, rollback, and development builds, see [Installation](docs/en/install.md).

## Update

Installed CLI releases include their verified updater:

~~~sh
maris update
maris update --dry-run
maris update --version 0.2.2
~~~

On Windows the same command launches the bundled PowerShell updater after the current Maris process exits. An older release that predates the updater must be installed once with the public installer above.

## Start

~~~sh
maris                 # open the normal TUI/app entry point
maris tui             # terminal UI
maris status          # saved state + runtime telemetry
maris doctor          # platform/audio diagnostics
maris devices         # outputs and inputs
maris --help          # every public CLI command
~~~

Explicit headless system capture requires routing authorization:

~~~sh
maris system --accept-routing
maris stop
~~~

For command details, use `maris <command> --help` or see [Commands & MCP](docs/reference/commands.md).

## Platform support

| Platform | System playback path | Application processing |
| --- | --- | --- |
| macOS 14.2+ | CoreAudio process tap | Selected apps or mixer channels |
| Windows | WASAPI process loopback | Selected apps, including child processes; mixer channels |
| Linux | Local PulseAudio/PipeWire-Pulse routing | PulseAudio-compatible apps or mixer channels |

Maris keeps processing local. Neural analysis, when bundled in a release, runs off the real-time audio callback. Audio playback does not depend on a network request.

## Main controls

The TUI and native menu expose the same guarded settings model:

- choose an output and listening preset;
- adjust device-specific bass, presence, air, softness, adaptive EQ, stereo/spatial controls, balance and compression;
- compare the enhanced path with its level-matched reference;
- undo the last compatible change;
- inspect current output, telemetry, correction, mixer state and diagnostics.

Device correction, listening preference, speech cleanup and mixer routing are separate features. Maris does not treat louder output as better sound.

## Documentation

- [Installation](docs/en/install.md)
- [User guide](docs/en/guide.md)
- [Commands & MCP](docs/reference/commands.md)
- [Architecture](docs/reference/architecture.md)
- [Audio verification](docs/reference/audio-quality.md)
- [Build & release](docs/development/releasing.md)
- [Third-party components](docs/reference/third-party.md)

Translated user guides are available in [简体中文](docs/zh-CN/index.md), [繁體中文](docs/zh-TW/index.md), [日本語](docs/ja/index.md), [Deutsch](docs/de/index.md), and [Español](docs/es/index.md). Engineering references are maintained in English.

## Development

~~~sh
cargo fmt --all --check
cargo check --locked
cargo test --locked --all-targets
cargo test --locked --all-targets --no-default-features
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tests/tooling -v
python3 scripts/site.py --check
~~~

Release building, release publication and ordinary push/PR CI are separate workflows. Hardware-specific behavior still requires real-device testing; synthetic and CI tests are not presented as physical-device acceptance.

## License and third-party notices

See [Third-party components](docs/reference/third-party.md) and the notices included in release packages.
