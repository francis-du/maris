<p align="center">
  <img src="docs/assets/wordmark.svg" alt="Maris" width="680">
</p>

<p align="center">System audio tuning for macOS, Windows and Linux.</p>

# Maris

Maris adjusts audio playing on your computer. Save different settings for headphones and speakers, apply headphone correction, and control bass, treble and stereo width from the terminal or menu bar.

For more detailed setups, the mixer connects multiple inputs to two separate outputs, with volume, EQ and compression for each channel. The command-line and MCP interfaces let other programs read status or make approved changes.

> **CLI/TUI 0.1.0 release preparation.** Native paths are implemented for macOS, Windows and Linux. The first command-line distribution requires exact-source native builds, archive checksums and installation/upgrade evidence. Physical-device and subjective listening acceptance remain unverified; GUI signing uses a separate distribution contract.

## Install

**A public application release is not available yet.** The commands below are for approved releases once they are published; an empty release list or missing website is not a working installer.

Normal installation downloads the precompiled package for the current platform from GitHub Releases. It does not install Rust or compile Maris. Development builds are described separately below.

**macOS / Linux**

~~~sh
curl -fsSL https://francis-du.github.io/maris/install.sh | bash
~~~

**Windows PowerShell**

~~~powershell
irm https://francis-du.github.io/maris/install.ps1 | iex
~~~

The installer chooses the package for your computer, verifies the download and executable with SHA-256, then installs it for your user account. It does not start audio, change system volume or default output, install a driver, edit PATH or disable platform security. See [installation](docs/en/install.md) for version pinning, offline installation, rollback and developer builds.

| Platform | Native system path | Application processing |
| --- | --- | --- |
| macOS 14.2+ | CoreAudio system audio capture | Selected apps or separate mixer channels |
| Windows | WASAPI application audio capture | Selected apps, including child processes; separate mixer channels |
| Linux | Local PulseAudio service, including PipeWire Pulse | Selected apps or separate mixer channels; PulseAudio-compatible apps only |

## Studio

![Maris Studio dashboard](docs/assets/studio-en.svg)

*Actual Maris TUI rendered with generated test audio. It is not a hardware-session screenshot.*

The main screen shows your output device, correction, sound controls, spectrum, calculated EQ curve and stereo levels. Music analysis, mixer status and diagnostics are also available without working through several tabs.

Useful controls:

- **O** — choose output; Enter confirms.
- **P** — choose a listening scene or EQ preset.
- **E** — settings; edits are staged before Apply.
- **B** / Space — reference comparison.
- **U** — undo the relevant change.
- Arrow keys or the visible **− / +** targets — adjust the selected listening parameter.
- **S** — stop processing.

Sound controls include EQ, dynamic reduction of strong frequency bands, optional virtual bass, stereo width and compression. Maris reserves digital headroom before boosting frequencies and limits output sample peaks. This is not hearing protection and does not limit peaks reconstructed between samples.

## Menu bar

![Native Maris menu in light and dark appearance, with selection and recovery states](docs/assets/menu-bar-en.svg)

The compact M responds to measured audio level and stays still with Reduce Motion. Select an output or preset, review it, then Apply; Compare and Undo remain directly available. Expired audio disables changes, and failed restoration keeps its diagnostic visible. These are actual macOS menu views rendered with isolated offline state; they do not establish audio-device acceptance.

## Device correction and local models

The AutoEq recommended-model index is pinned and shipped with the source. Release builds assemble the matching parametric profiles at build time and embed the verified profile pack, so normal playback does not fetch AutoEq data.

The release workflow includes MusicNN for local music recognition. A plain `cargo build --release` does not include its weights. Missing weights or a model-loading error leave ordinary audio processing available; the analysis status explains the failure. MusicNN runs separately from playback and does not upload raw audio. RNNoise can reduce speech noise when explicitly enabled; it is not used for ordinary music. Model sources, permissions and remaining checks are in [third-party components](docs/reference/third-party.md) and [model checks](docs/development/models.md).

## Mixer

The mixer accepts multiple audio devices or applications and sends them to two independent outputs. Each channel has volume, pan, mute, solo, output selection, EQ and compression. It accounts for different device sample rates and clocks. Changing audio routes requires confirmation; shutdown attempts to restore the original app output. Platform-specific recovery limitations remain part of the release checks.

## Automation

The JSON CLI and MCP server use the same settings checks and undo as the interface. MCP starts read-only; permission to change settings must be enabled explicitly. It cannot execute arbitrary shell commands or change system volume.

~~~sh
maris --help
maris --json status
maris tui
~~~

## Documentation

[English](docs/en/index.md) · [简体中文](docs/zh-CN/index.md) · [繁體中文](docs/zh-TW/index.md) · [日本語](docs/ja/index.md) · [Deutsch](docs/de/index.md) · [Español](docs/es/index.md)

[User guide](docs/en/guide.md) · [Commands & MCP](docs/reference/commands.md) · [Architecture](docs/reference/architecture.md) · [Audio tests](docs/reference/audio-quality.md) · [Build & release](docs/development/releasing.md)

The website is generated from the same Markdown and locale sources. Engineering references are maintained in English; customer-facing pages are translated.

## Development

There is no separate run script.

~~~sh
cargo build --release --locked
cargo fmt --all --check
cargo test --locked --all-targets
cargo test --locked --all-targets --no-default-features
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tests/tooling -v
python3 scripts/site.py --check
~~~

Source is grouped by responsibility under src/: audio backends, DSP, device data, mixer, models, tuning, UI and control APIs each have their own module directory. See the [source layout](docs/reference/architecture.md#source-layout).

## Release policy

Push/PR checks, native application builds, documentation deployment and public release are separate GitHub workflows. Passing offline tests does not establish Bluetooth behavior, acoustic performance or listening preference.

The public release remains blocked until the [release requirements](docs/development/requirements.json) and native acceptance checks are complete.
