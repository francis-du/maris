# Build, verification and release

Repository: https://github.com/francis-du/maris
Planned project documentation URL: https://francis-du.github.io/maris/
A workflow file or expected URL is not evidence that a remote workflow has run or a site is live.

## Separate workflows

| File | Trigger | Responsibility | Publication permission |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | main push, PR, manual | Source audit, docs links, formatting, default/minimal tests, Clippy on macOS/Linux/Windows | Read-only; no release build/upload |
| `.github/workflows/build.yml` | Manual only | Full checks, 200 offline rounds, native x86_64/ARM64 artifacts for macOS/Linux/Windows, isolated installation and checksums | Read-only repository access; workflow artifacts only |
| `.github/workflows/pages.yml` | Documentation changes on main, manual | Static documentation validation, build and deployment | Pages/OIDC write only on the separate deploy job |

All action references are full commit SHAs. Dependabot proposes updates. No workflow uses pull_request_target, commits generated build directories, or treats a branch push as authorization to publish an application release. Rust 1.90.0 is the declared toolchain for CI. There is no claim that an unobserved CI matrix has passed.

## Source hygiene before committing

```sh
python3 scripts/source_audit.py --manifest
git add --pathspec-from-file=.maris-review/source-files.nul --pathspec-file-nul
git diff --cached --stat
git diff --cached --check
```

Inspect the staged paths and contents before commit. The allowlist includes product source, tests, install/build scripts, workflows, documentation, and retained eqMac attribution. Versioned `.wcode/project.yaml` and `.wcode/design/` are included as engineering contracts. Other local `.wcode` execution state, target, dist, _site, verification logs, model caches, local settings, audio recordings, credentials and editor files are excluded. The audit rejects unexpected included paths, symlinks, large text inputs, token/private-key patterns and personal absolute paths. It is a bounded credential check, not a guarantee that every kind of secret can be detected. Do not blanket-add or delete unknown changes.

## Verification

```sh
python3 -m unittest discover -s tests/tooling -v
python3 scripts/site.py --check
bash -n install.sh
cargo fmt --all --check
cargo test --all-targets --locked
cargo test --all-targets --no-default-features --locked
cargo clippy --all-targets --locked -- -D warnings
python3 scripts/verify_rounds.py --rounds 200
cargo build --release --locked
```

Each of the 200 rounds executes a uniquely seeded real DSP/draft transaction case and one rotating existing regression suite. The matrix varies 44.1/48/96/192 kHz, EQ presets, listening scenes, generated finite/nonfinite frames and revisioned draft application. The rotating suites cover configuration input/layout, click delivery, menus, routing messages, preference misuse, numerical feature ablation, audio math, scenes and installation. On macOS, installer cases invoke the real script against isolated copies of the real Maris executable; they do not install into the user's Applications or start system audio.

The runner compiles test binaries once, records each seed/exit status/duration/log hash, and uses four bounded workers by default (`--jobs 1` selects serial execution). Each process has isolated temporary test state. A failed case stops new batches; already-running cases in its small batch finish and stay recorded. Source drift invalidates the run. A command interrupted before its final report is incomplete, not a pass. Reports live in `.maris-review/verification/<run>/report.json`; failed runs remain separate. A new source change invalidates prior report applicability. This is **not** 200 independent audits, 200 full-suite runs, hardware hotplug acceptance or a listening test. Full baseline checks and real-device gates remain separate. Ignored network tests are not counted as passed.

### Bounded foreground verification

Hosts with short command time limits may use `python3 scripts/verify_rounds.py --rounds 200 --batch-size 40`, then `python3 scripts/verify_rounds.py --resume .maris-review/verification/<returned-run-directory> --batch-size 40` until the original total is reached. Each foreground batch stops normally and retains its original seed sequence. Resume checks the source digest, compiled binary hashes and every retained complete passing step/log; changed, missing, failed or reordered evidence is rejected. It never imports an earlier timed-out run without a valid checkpoint or silently retries a recorded failed case.

Partial reports explicitly contain `passed: false` and `complete: false`, even when that batch exits successfully. Only the final complete run can satisfy the release-report gate. A held `.batch.lock` is not automatically removed: an interrupted process must be inspected first. The normal CI command without a batch size still runs all 200 rounds in one invocation. These are seeded offline cases plus rotating regression suites, not 200 independent reviews, full-suite repetitions or hardware/listening acceptance.

## Local artifact build

```sh
python3 scripts/package.py --local
```

This rebuilds the native optimized executable using the lockfile and makes a source-hash-named ZIP (macOS/Windows) or tar.gz (Linux), plus a SHA-256 checksum in `dist/packages`. macOS contains Maris.app; Linux/Windows contain an architecture-validated Maris/bin payload and its package marker. Platform-appropriate installation scripts, instructions and eqMac provenance are included. BUILD.json records source/executable hashes, platform, architecture and local-development status; it never claims signing, notarization or commercial release approval. The command does not sign, upload, install, or start audio. Run `python3 scripts/package_smoke.py` afterward to validate archive paths/checksums and perform a dry run plus real installation under isolated temporary directories; only the installed test copy's `--version` is executed. A full dependency/asset notice audit is still required before distribution as a public release. Do not turn a local archive into a signed-release claim by renaming it.

The manual GitHub build can be started from Actions → Build application artifacts → Run workflow. Separate native jobs cover macos-15, macos-15-intel, ubuntu-24.04, ubuntu-24.04-arm, windows-2025 and windows-11-arm. Runner execution results, not matrix declarations or cross-compiled headers, establish which targets actually passed. Build success does not publish a GitHub Release.

## GitHub Pages

Choose Settings → Pages → Source: GitHub Actions once, then run Documentation Pages. Initial Pages enablement needs repository administration/Pages permission and cannot be performed by the default workflow GITHUB_TOKEN; a local SSH Git push does not provide GitHub API login. Do not embed an administrator token in a workflow to work around missing access. The build uses Python's standard library to validate local links, anchors, relative project-site URLs, basic accessible HTML structure and the explicit static file set. It requires no JavaScript, external font, analytics service, theme download or third-party Python package.

```sh
python3 scripts/site.py
python3 -m http.server 8000 --directory _site
```

Review the generated site locally. A successful Pages deployment, not simply pushing the YAML, establishes availability. Its expected project path is `/maris/`. The deployment has its own `github-pages` environment and least-privilege token.

## Public application release gate

Do not publish until all applicable PRODUCT_GATES.md requirements are actually met: exact-revision CI, recorded automated checks, a documented headphone/Bluetooth matrix, level-matched output/preset/A-B listening acceptance, dependency/model/asset redistribution review, a deliberate project-license decision, Developer ID signing, notarization, stapling, and final archive verification. No credentials or signing identities belong in Git.

`scripts/release_gate.py` validates the current-source round report, every requested feature's acceptance record and the exact final payload. Publisher verification is platform-specific: macOS requires codesign/Gatekeeper/stapling; Windows requires valid PE identity and Authenticode; Linux requires a detached signature of the reviewed payload digest with an explicitly reviewed public keyring and key-currentness record. Each gate runs on the native platform. Windows/Linux packaging does not inherit macOS acceptance. The tool does not create evidence, grant approval, modify signatures, or publish. A missing/failed/changed input fails closed. Source publication, local workflow artifacts and commercial distribution are different decisions.

```sh
python3 scripts/release_gate.py \
  --report .maris-review/verification/RUN/report.json \
  --approval .maris-review/release-approval.json \
  --bundle dist/Maris.app
```

Approval JSON needs the current `source_sha256`, the final signed/stapled bundle's `bundle_sha256` (computed by `release_gate.bundle_digest`), a nonempty `reviewed_by`, and evidence objects for `real_device_matrix`, `listening_validation`, `dependency_notices`, `project_license`, and `remote_ci`. Each evidence object has `accepted: true`, a repository-relative `path` to the reviewed record and its `sha256`. Do not fill these fields with invented acceptance. The bundle digest binds regular file contents and permission bits; linked/special files are rejected. The gate checks the Cargo/bundle version and rechecks the bundle and source after signature validation. The repository deliberately ships no pre-approved acceptance file.

After successful review, the owner may sign the final bundle with a protected Developer ID identity, submit it with Apple's notarytool, staple the accepted ticket, verify it again, then create a version tag and a reviewed GitHub Release. Never clear quarantine or disable Gatekeeper to make the gate pass. No automation currently publishes an application solely because 200 rounds passed.

## Online installation is the normal distribution path

The root Bash and PowerShell installers now download precompiled native GitHub release assets by default. Rust/Cargo compilation is confined to explicit developer `--build` / `-Build`. The offline transaction helpers live in `scripts/install_macos.sh`, `scripts/install_linux.sh`, and the local-input path in `install.ps1`. Canonical customer instructions are [installation](../en/install.md), with five additional translated versions.

The manual native CI matrix runs `scripts/release_bundle.py --candidate` after package and install checks. `package.py` owns the optimized build; the workflow does not issue a duplicate release build. Kit preparation wraps that compiled current-source payload without compiling again. Before any manifest is written, the collector opens each real TAR/ZIP, rejects unsafe entries, reads its embedded release/native identity, hashes its actual executable and matches these against the descriptor. Recomputing an outer archive checksum cannot make arbitrary bytes or a different executable a valid kit.

The assembly job retrieves only the current run's six native artifacts. All descriptors must match the run ID, commit, repository and current checked-out source, and must cover exactly six distinct targets. It writes a **candidate** `maris-release.tsv`; a normal online installer refuses this channel. Actions artifacts are the authenticated development channel, not the stable public download URL.

After every actual product requirement and native signing/acceptance gate is satisfied, prepare an approved kit on each native target using the final reviewed payload, never an unsigned rebuild:

```sh
python3 scripts/release_bundle.py \
  --bundle PATH_TO_FINAL_NATIVE_PAYLOAD \
  --report .maris-review/verification/RUN/report.json \
  --approval .maris-review/release-approval.json
```

Linux also supplies `--signature`, `--digest-file` and `--keyring` as required by the native release gate. The gate runs before packaging and again on the extracted archive. The six approved kits must have the same source and version before their records can be combined:

```sh
python3 scripts/release_bundle.py --collect dist/approved-targets --output dist/approved-manifest/maris-release.tsv
```

This does not upload or publish anything. Only after review may the owner publish the six exact archive names plus the stable manifest under `vVERSION`. Unix online kits are `.tar.gz`; Windows kits are `.zip`. The default bootstrap requests `releases/latest/download/maris-release.tsv`, then uses only version-pinned URLs. Failed downloads never trigger compilation, security-policy changes or candidate fallback.

Approval now additionally requires an explicit record for every ID in `docs/development/requirements.json`; each has verified status plus a hash-bound acceptance entry in approval JSON's `requirements` map. Deleting, omitting or merely hiding an unfinished capability is rejected. Website, Logo and online-installer work do not clear the remaining hardware, model, mixer or platform requirements.

## Primary platform references

- GitHub Pages custom workflows: https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages
- GitHub-hosted runner labels: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- Checkout action: https://github.com/actions/checkout
- Artifact action: https://github.com/actions/upload-artifact
- Apple notarization: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution

Actions/runner details were checked on 2026-10-01. The full project trust and DSP boundaries remain in [architecture](../reference/architecture.md) and [audio-quality evidence](../reference/audio-quality.md). Installer transport tests use isolated responses; native archive/build checks and real published-release installation are separate evidence.
