# Build, verification and release

Repository: https://github.com/francis-du/maris
Public documentation URL: https://maris.francis.run/
A workflow file or expected URL is not evidence that a remote workflow has run or a site is live.

## Separate workflows

| File | Trigger | Responsibility | Publication permission |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | main push, PR, manual | Source audit, docs links, formatting, default/minimal tests, Clippy on macOS/Linux/Windows | Read-only; no release build/upload |
| `.github/workflows/build.yml` | Release published, manual, or guarded `release-validation/**` push at the exact current `main` SHA | Full checks, 200 offline rounds, native x86_64/ARM64 artifacts for macOS/Linux/Windows, isolated installation and checksums | Read-only repository access; workflow artifacts only |
| `.github/workflows/publish-release.yml` | Guarded `release-publish/vX.Y.Z` push at the exact current `main` SHA after a successful same-commit validation build | Create the owned tag and draft, call the asset workflow, promote only after all public installation checks, and roll back the owned release on failure | Contents write for the recorded release/tag; Actions read for preflight |
| `.github/workflows/release-assets.yml` | Reusable call from stable publication, or successful `Build application artifacts` completion | Bind the exact run, source, tag and release; validate and upload eight CLI assets; run twelve native installation jobs for a new release, or six for an existing published stable release | Contents write only for the verified matching Release |
| `.github/workflows/pages.yml` | Documentation changes on main, manual | Static documentation validation, build and deployment | Pages/OIDC write only on the separate deploy job |

All action references are full commit SHAs. Dependabot proposes updates. No workflow uses pull_request_target or commits generated build directories. Ordinary main/PR checks do not publish. A `release-validation/**` push produces workflow artifacts after checking the current main SHA; a successful build with no matching version tag exits the automatic publication path without publishing. The explicit `release-publish/vX.Y.Z` branch is the guarded stable-release entry point described below.

Rust 1.94.0 is the declared toolchain for CI, matching the minimum supported version in Cargo.toml. Candle 0.11 uses ARM64 float16 vector types that are unavailable on Rust 1.90. The first macOS CI run failed there before Maris tests could execute. The type was stabilized in [Rust 1.94](https://doc.rust-lang.org/stable/core/arch/aarch64/struct.float16x8_t.html); do not use unstable compiler overrides to hide this requirement. There is no claim that an unobserved CI matrix has passed.

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

Partial reports explicitly contain `passed: false` and `complete: false`, even when that batch exits successfully. Only the final complete run can satisfy the release-report gate. A held `.batch.lock` is not automatically removed: exceptions, interrupts and failed evidence writes retain it, even when a test already ran or an older report says it passed. Inspect the interrupted directory and preserve its evidence; use a new run rather than deleting the lock to retry unrecorded work. Only a normally settled batch removes its own lock. Release preflight rejects a lock or invalidation marker before and after reading all retained logs. The normal CI command without a batch size still runs all 200 rounds in one invocation. These are seeded offline cases plus rotating regression suites, not 200 independent reviews, full-suite repetitions or hardware/listening acceptance.

## Local artifact build

```sh
python3 scripts/package.py --local
```

This rebuilds the native optimized executable using the lockfile and makes a source-hash-named ZIP (macOS/Windows) or tar.gz (Linux), plus a SHA-256 checksum in `dist/packages`. macOS contains Maris.app; Linux/Windows contain an architecture-validated Maris/bin payload and its package marker. Platform-appropriate installation scripts, instructions and eqMac provenance are included. BUILD.json records source/executable hashes, platform, architecture and local-development status; it never claims signing, notarization or commercial release approval. The command does not sign, upload, install, or start audio. Run `python3 scripts/package_smoke.py` afterward to validate archive paths/checksums and perform a dry run plus real installation under isolated temporary directories; only the installed test copy's `--version` is executed. A full dependency/asset notice audit is still required before distribution as a public release. Do not turn a local archive into a signed-release claim by renaming it.

The manual GitHub build can be started from Actions → Build application artifacts → Run workflow. Publishing a GitHub Release also starts this build when GitHub delivers the release event. Separate native jobs cover macos-15, macos-15-intel, ubuntu-24.04, ubuntu-24.04-arm, windows-2025 and windows-11-arm. Runner execution results, not matrix declarations or cross-compiled headers, establish which targets actually passed. A manual build is not the preflight selected by `publish-release.yml`: a new stable release requires a successful same-commit push build on `release-validation/**`. Build success alone does not create a tag or Release.

## GitHub Pages

Choose Settings → Pages → Source: GitHub Actions once, then run Documentation Pages. Initial Pages enablement needs repository administration/Pages permission and cannot be performed by the default workflow GITHUB_TOKEN; a local SSH Git push does not provide GitHub API login. Do not embed an administrator token in a workflow to work around missing access. The build uses Python's standard library to validate local links, anchors, relative project-site URLs, basic accessible HTML structure and the explicit static file set. It requires no JavaScript, external font, analytics service, theme download or third-party Python package.

```sh
python3 scripts/site.py
python3 -m http.server 8000 --directory _site
```

Review the generated site locally. A successful Pages deployment, not simply pushing the YAML, establishes availability. The configured custom domain serves the site from `/`; public links use `https://maris.francis.run/` directly so installers do not depend on an HTTP redirect from the legacy GitHub Pages hostname. The deployment has its own `github-pages` environment and least-privilege token.

## CLI/TUI release contract

The CLI/TUI distribution introduced in version 0.2.0 uses portable archives on six native targets. This contract requires exact-source CI, the recorded 200 offline rounds, native executable validation, actual isolated installation and startup, retained upgrade backups, a rejected corrupt upgrade that preserves the installed version and user data, complete native runtime notice inventories, and archive/executable SHA-256. The release metadata states that these files are unsigned and that physical-device and subjective listening acceptance are unverified. It does not mark unfinished product requirements as verified.

`package.py --local --cli` compiles once and creates `Maris/bin/maris` (or `maris.exe`) with a `maris-package-v2` identity ending in `cli`. On macOS this is a portable command-line payload, not a `.app`. `package_smoke.py` exercises its real native installer and installed `--version`. `release_bundle.py --cli` reads the actual round logs, install report, notice index and final archive bytes; it creates a stable CLI kit without signing or publishing. The assembly job requires all six artifacts from its exact CI run and commit.

The CLI manifest uses `maris-release-v2` with `interface\tcli`; its embedded kit and payload identities must match. GUI v1 payloads cannot be relabeled to avoid their publisher checks. CLI installation verifies HTTPS acquisition, archive and binary hashes, native executable type/architecture, paths, size bounds and atomic replacement. It does not alter script policy, quarantine, PATH, services, permissions or audio routing. macOS/Linux use `~/.local/lib/maris` and an owned `~/.local/bin/maris` launcher; Windows uses `%LOCALAPPDATA%/Programs/Maris`.

Project licensing is recorded as declared if a root `LICENSE` exists, otherwise as unspecified without inventing a new grant. The CLI software gate preserves that status and does not choose a license on the owner's behalf. The separate GUI application gate below still requires owner-reviewed `project_license` evidence through `release_gate.py`. Each dependency retains its own terms. `resources/notices/index.json` binds the actual selected runtime licenses and source pointers; two upstream crates that omit standalone notices retain their published MIT/author declarations explicitly, as described in `third_party/rust-notices/SOURCE.md`.

### Publish v0.2.3 through the guarded workflow

The following steps describe the required process; this document is not evidence that v0.2.3 has been built, validated or published.

1. Add reviewed `.github/release-notes/vX.Y.Z.md` for that version; the publisher reads it from the exact source commit and prepends it to the generated changelog. Update the package version in `Cargo.toml`, the root `maris` package in `Cargo.lock`, the bundle version metadata and current updater examples together. Native menu illustration provenance includes `Cargo.toml`, so this change also requires `python3 scripts/capture_menu_docs.py --refresh` on native macOS. Review the freshly rendered SVGs and capture manifest and retain the actual probe evidence; editing only the recorded source hash is not a refresh. Merge the preparation change and verify both its PR checks and the independent main checks for the resulting commit. Changing the version or any audited source file changes the source digest: older archives, round reports and manifests cannot be relabeled as v0.2.3 evidence.
2. Push a `release-validation/**` branch, for example `release-validation/v0.2.3-SHORT_SHA`, at exactly that current main commit. Wait for the complete `Build application artifacts` workflow: dependency audit, all six native jobs and `assemble-cli`. The native jobs perform the full tooling/Rust checks, pinned model and AutoEq checks, applicable menu/Linux integration checks, 200 recorded offline rounds, optimized packaging, isolated install/upgrade/recovery validation and CLI kit preparation. The collector requires all six kits to match this run, commit, repository, version and audited source. Record actual terminal results and artifact bytes; workflow artifacts expire after fourteen days.
3. After that preflight succeeds, push `release-publish/v0.2.3` at the same still-current main commit. `Publish stable release` has no manual-dispatch entry or user-supplied inputs. It derives the version from the branch, checks it against Cargo, selects a successful push build from `release-validation/**` with the exact commit, and rejects an existing tag or Release. Do not pre-create or move `v0.2.3`; the workflow owns creation and recovery.
4. The publisher rechecks main, creates the version tag and a draft Release, records its numeric release ID, and checks the tag and main again. It calls `release-assets.yml` with `source_run_id`, `source_sha`, `release_tag` and `release_id`. The reusable workflow verifies that the build belongs to this repository and source and that the selected Release is this invocation's unpublished draft.
5. The asset workflow revalidates the stable CLI manifest and the bytes of all six archives from the selected run, then uploads the six versioned archives, `maris-release.tsv` and `SHA256SUMS`: eight installer-facing assets. Existing attachments can be reused only when their downloaded bytes match; conflicting bytes are not blindly overwritten. The workflow downloads the resulting assets and checks their bytes again before exposing the Release.
6. The owned draft becomes a public prerelease with `make_latest=false`. Twelve native public-install jobs cover macOS, Linux and Windows on x86_64 and ARM64 twice: once using the exact release-source bootstrap and once using the current public default-branch bootstrap. Each uses the normal online `install.sh` or `install.ps1` against the version-pinned public Release, installs under an isolated prefix and executes the installed `--version`. These checks do not activate user audio or establish physical-device or listening acceptance.
7. Only after all twelve installation jobs succeed does the workflow recheck current main, tag SHA, release ID and public-prerelease state, then promote the Release to stable and latest. Main advancing before promotion is a rejection, so finish or restart validation for the selected source before attempting another release. Failure cleanup is restricted to the recorded release identity, recorded attachment names and a tag that still points at the owned source; it does not delete replacement identities. Inspect the actual cleanup result after a failed or cancelled attempt. A draft, exposed prerelease or incomplete workflow is not a completed stable release.

The automatic build-completion path remains separate. It requires exactly one matching version tag and an already-published stable Release, revalidates the exact successful build and runs six native jobs using the current public bootstrap. It does not create a new tag or Release and is not a substitute for the guarded new-release process. Both publication paths serialize asset upload and cleanup across source revisions.

## GUI application release gate

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

The legacy GUI candidate path runs `scripts/release_bundle.py --candidate` after package and install checks. `package.py` owns the optimized build; the workflow does not issue a duplicate release build. Kit preparation wraps that compiled current-source payload without compiling again. Before any manifest is written, the collector opens each real TAR/ZIP, rejects unsafe entries, reads its embedded release/native identity, hashes its actual executable and matches these against the descriptor. Recomputing an outer archive checksum cannot make arbitrary bytes or a different executable a valid kit.

CLI and GUI kit preparation commit the archive and then its descriptor with separate no-overwrite hard links. A pre-existing conflicting descriptor is rejected before adding the archive. Once linked, an archive is never deleted by a failed preparer: another concurrent preparer may already have committed a matching descriptor. A record-write failure or process interruption can therefore retain an orphan archive; retry accepts it only when the rebuilt bytes match exactly. A mismatching retry remains rejected without replacing either final path. An orphan is not a completed kit and cannot satisfy the six-target collector or release approval. This local recovery rule does not publish assets or change the separate public-release cleanup policy.

The commit boundary hashes the prepared archive against its reviewed digest and freezes the descriptor bytes before linking. Reads validate regular-file descriptors and their matching path identities, stream archives in bounded chunks, and enforce the existing 128 MiB archive and 32 KiB descriptor limits. Linked final objects are checked again before success; changing both hard-linked record paths cannot redefine the frozen comparison baseline. Concurrent identical hard links may change link count and ctime without changing payload bytes, so those fields are not used as content identities. Failed integrity checks preserve shared paths rather than deleting a peer's files. These checks do not make a two-file transaction atomic or files immutable after return: the collector and native release gates must still independently revalidate the actual artifacts.

For legacy GUI candidates, the assembly job retrieves only the current run's six native artifacts. All descriptors must match the run ID, commit, repository and current checked-out source, and must cover exactly six distinct targets. It writes a **candidate** `maris-release.tsv`; a normal online installer refuses this channel. Actions artifacts are the authenticated development channel, not the stable public download URL.

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

This collector does not upload or publish anything by itself. The current GitHub build and publication workflows select the CLI contract above; preparing approved GUI kits does not make them eligible for that CLI workflow. Unix online kits are `.tar.gz`; Windows kits are `.zip`. The default bootstrap requests `releases/latest/download/maris-release.tsv`, then uses only version-pinned URLs. Failed downloads never trigger compilation, security-policy changes or candidate fallback.

Approval now additionally requires an explicit record for every ID in `docs/development/requirements.json`; each has verified status plus a hash-bound acceptance entry in approval JSON's `requirements` map. Deleting, omitting or merely hiding an unfinished capability is rejected. Website, Logo and online-installer work do not clear the remaining hardware, model, mixer or platform requirements.

## Primary platform references

- GitHub Pages custom workflows: https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages
- GitHub-hosted runner labels: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- Checkout action: https://github.com/actions/checkout
- Artifact action: https://github.com/actions/upload-artifact
- Apple notarization: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution

Actions/runner details were checked on 2026-10-01. The full project trust and DSP boundaries remain in [architecture](../reference/architecture.md) and [audio-quality evidence](../reference/audio-quality.md). Installer transport tests use isolated responses; native archive/build checks and real published-release installation are separate evidence.
