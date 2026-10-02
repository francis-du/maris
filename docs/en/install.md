# Installation

The installer downloads a native package built by GitHub CI. You do not need a source checkout, Rust or a compiler.

> Maris is in development. These commands require the documentation site and an approved GitHub Release to be available. If either download fails, installation stops; it does not build from source.

## Install {#online}

**macOS / Linux**

```sh
curl -fsSL https://francis-du.github.io/maris/install.sh | bash
```

**Windows PowerShell**

```powershell
irm https://francis-du.github.io/maris/install.ps1 | iex
```

The Unix command runs the downloaded script directly and asks for confirmation through the terminal before any package download. PowerShell also asks before replacing the current installation. No administrator privileges are needed. The installer does not start audio, change PATH, install drivers or modify system volume/default output.

To inspect the script first, download [install.sh](https://francis-du.github.io/maris/install.sh) or [install.ps1](https://francis-du.github.io/maris/install.ps1), then run `bash install.sh` or `.\install.ps1`. A script-policy or signature rejection should be investigated, not bypassed.

## Options {#options}

| Purpose | macOS / Linux | Windows |
| --- | --- | --- |
| Show the plan without network or file changes | `bash install.sh --dry-run` | `.\install.ps1 -DryRun` |
| Install a specific published version | `bash install.sh --version v1.2.3` | `.\install.ps1 -Version v1.2.3` |
| Choose a user-owned destination | `bash install.sh --prefix "$HOME/Audio Tools"` | `.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools"` |
| Skip the installation question | `bash install.sh --yes` | `.\install.ps1 -Yes` |

Replace the example version with a published tag. Without a version, the installer resolves the latest stable release once and downloads the package from that fixed tag.

## Platforms and locations {#platforms}

| System | Architectures | Default installation |
| --- | --- | --- |
| macOS 14.2+ | x86_64, ARM64; detects Rosetta | `~/Applications/Maris.app` |
| Linux | x86_64, ARM64 | `~/.local/lib/maris`, with a launcher at `~/.local/bin/maris` |
| Windows | x86_64, ARM64 | `%LOCALAPPDATA%/Programs/Maris` |

Unix installation uses Bash, curl, tar, gzip and a SHA-256 tool. Linux needs its native runtime libraries and `psmisc` for checking open executables during upgrades. Windows uses PowerShell 5.1+ and built-in .NET HTTP/ZIP support.

All three system-audio paths are implemented: CoreAudio process tap on macOS, WASAPI process loopback on Windows and a local PulseAudio-compatible path (including PipeWire-Pulse) on Linux. Installation does not start any of them. Native CI, clean-system, real-device and long-running acceptance remain separate [release requirements](status.md).

## Open, upgrade or remove {#recovery}

Close the installed Maris before an upgrade. The installer will not terminate a running audio process. It keeps the previous installation in a printed `.maris-backup.*` path and attempts to restore it if replacement fails. Preferences and headphone correction are retained.

Rerun the installer to upgrade. For rollback, select a previously published version or use a complete local installation kit with `--from` / `-From` pointing to the retained backup. After an interrupted installation, inspect the printed paths and running processes before removing a lock.

On macOS, open Maris.app when ready and grant the audio-recording permission. For CLI use, call the executable at the location printed by the installer. `--help` does not start capture. Closing the TUI leaves an existing session running; `maris stop` stops it.

To uninstall, stop Maris and remove only its installed app/payload and owned launcher. Keeping the preferences allows a later installation to reuse them.

## Verification and errors {#verification}

The installer checks release status, OS, architecture, version, download length, archive paths and archive/executable SHA-256. It rejects candidate packages, unsafe archive entries and mismatched files. Downloads have HTTPS, time and size limits. macOS uses signature/Gatekeeper checks; Windows uses Authenticode. Linux checks the executable digest from the release manifest. Checksums alone do not independently identify a publisher.

A missing release or HTTP error leaves the current installation unchanged. A checksum, archive or signature failure requires checking the release source. A busy-installation error requires closing Maris or inspecting its lock. Do not turn off OS security protections to install a rejected package.

## CI artifacts and source builds {#development}

Temporary Actions artifacts are for testing; the normal installer uses approved GitHub Release attachments. Builds, source push, Pages deployment and application release are separate operations.

An extracted local kit can be installed with `--from` / `-From`. Only an explicit developer `--build` / `-Build` needs Rust 1.90+ and the platform compiler. `--allow-unsigned` / `-AllowUnsigned` applies only to trusted local development packages, never normal online downloads. See [build and release](../development/releasing.md) for those commands and the remaining release checks.
