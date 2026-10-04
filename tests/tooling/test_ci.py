"""Keep declared Rust requirements and native CI jobs in agreement."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def gate_bash(*, windows=os.name == 'nt', environment=None):
    if not windows:
        return shutil.which('bash')
    environment = os.environ if environment is None else environment
    candidates = [Path(environment[key]) / 'Git/bin/bash.exe'
                  for key in ('ProgramW6432', 'ProgramFiles', 'ProgramFiles(x86)') if environment.get(key)]
    git = shutil.which('git')
    if git:
        candidates.append(Path(git).parent.parent / 'bin/bash.exe')
    # Windows' PATH may resolve bash.exe to the WSL launcher, which is not the
    # Git Bash interpreter used by Actions' shell: bash workflow steps.
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    return None


BASH = gate_bash()


def minimum_rust():
    # This project has one package. Keep the check dependency-free on Python 3.10.
    text = (ROOT / 'Cargo.toml').read_text(encoding='utf-8')
    versions = re.findall(r'^rust-version\s*=\s*"(\d+\.\d+)"\s*$', text, re.M)
    if len(versions) != 1:
        raise ValueError('Expected one explicit Rust requirement in Cargo.toml')
    return versions[0]


class ToolchainRequirements(unittest.TestCase):
    @unittest.skipUnless(BASH, 'Git Bash bundle gate execution required')
    def test_bundle_gate_stops_before_autoeq_when_the_model_command_fails(self):
        text = (ROOT / '.github/workflows/build.yml').read_text(encoding='utf-8')
        step = re.search(r'      - name: Verify bundled semantic model and AutoEq profile pack\n'
                         r'((?:        .*\n)+)', text)
        self.assertIsNotNone(step)
        self.assertIn('        shell: bash\n', step[1])
        block = re.search(r'        run: \|\n((?:          .*\n)+)', step[1])
        self.assertIsNotNone(block)
        script = '''cargo() {
    case "$*" in *models::musicnn::*) return 37 ;; esac
    printf 'later AutoEq check ran\\n'
}
''' + '\n'.join(line[10:] for line in block[1].splitlines())
        result = subprocess.run([BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                                capture_output=True, text=True, encoding='utf-8', timeout=10)
        self.assertEqual(result.returncode, 37, result.stdout + result.stderr)
        self.assertNotIn('later AutoEq check ran', result.stdout)

    def test_windows_dependency_gate_uses_git_bash_instead_of_a_wsl_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            programs = Path(directory)
            actual = programs / 'Git/bin/bash.exe'
            actual.parent.mkdir(parents=True)
            actual.write_bytes(b'Git Bash fixture')
            with mock.patch('shutil.which', side_effect=lambda name: 'WSL-launcher.exe' if name == 'bash' else None):
                self.assertEqual(gate_bash(windows=True, environment={'ProgramFiles': directory}), str(actual))
                actual.unlink()
                self.assertIsNone(gate_bash(windows=True, environment={'ProgramFiles': directory}))

    @unittest.skipUnless(BASH, 'Git Bash dependency gate execution required')
    def test_dependency_gates_reject_vulnerabilities_in_large_trees(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            cargo = temporary / 'cargo'
            cargo.write_text('#!/bin/sh\ncat tree.txt\n', encoding='utf-8', newline='\n')
            cargo.chmod(0o755)
            tree = temporary / 'tree.txt'
            environment = dict(os.environ, RUNNER_OS='Regression fixture')
            for name in ('ci.yml', 'build.yml'):
                text = (ROOT / '.github/workflows' / name).read_text(encoding='utf-8')
                block = re.search(r'      - name: Reject vulnerable dependencies reachable on this native target\n'
                                  r'        shell: bash\n        run: \|\n((?:          .*\n)+)', text)
                self.assertIsNotNone(block, name)
                # Git Bash needs its own path syntax; $PWD also preserves spaces.
                script = 'export PATH="$PWD:$PATH"\n' + '\n'.join(
                    line[10:] for line in block[1].splitlines())
                for dependency, rejected in [('glib v0.18.5', True), ('ringbuf v0.4.8', True),
                                             ('glib v0.21.5\nringbuf v0.5.2', False)]:
                    with self.subTest(workflow=name, dependency=dependency):
                        # Exceed pipe capacity so grep -q's early exit cannot hide SIGPIPE.
                        tree.write_text(dependency + '\n' + 'dependency v1.0.0\n' * 20_000, encoding='utf-8', newline='\n')
                        result = subprocess.run([BASH, '-c', script], cwd=temporary, env=environment,
                                                capture_output=True, text=True, encoding='utf-8', timeout=10)
                        self.assertEqual(result.returncode, 1 if rejected else 0, result.stdout + result.stderr)
                        if rejected:
                            self.assertIn('reachable on Regression fixture', result.stderr)

    def test_native_workflows_test_the_declared_minimum_rust_version(self):
        minimum = minimum_rust()
        self.assertRegex(minimum, r'^\d+\.\d+$')
        for name in ('ci.yml', 'build.yml'):
            with self.subTest(workflow=name):
                text = (ROOT / '.github/workflows' / name).read_text(encoding='utf-8')
                pinned = re.findall(r'^\s+RUSTUP_TOOLCHAIN: (\d+\.\d+\.\d+)\s*$', text, re.M)
                self.assertEqual(len(pinned), 1, 'Use one exact stable Rust version')
                self.assertEqual('.'.join(pinned[0].split('.')[:2]), minimum)
                installed = re.findall(r'rustup toolchain install (\d+\.\d+\.\d+)\b', text)
                self.assertEqual(installed, pinned, 'Install the same compiler that runs Cargo')
                self.assertNotIn('RUSTC_BOOTSTRAP', text)
                self.assertNotIn('continue-on-error', text)

    def test_push_checks_execute_the_bundled_model_not_only_the_absent_model_case(self):
        text = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
        self.assertRegex(text, r"MARIS_BUNDLE_SMALL_MODELS:\s*'1'\s*\n\s*run: \|\s*\n\s*cargo test --locked --lib models:: -- --nocapture")
        self.assertNotIn('--skip', text)

    def test_inline_workflow_commands_do_not_form_yaml_mappings(self):
        # A plain YAML scalar cannot contain a colon followed by whitespace.
        # Rust test filters ending in :: therefore need a quoted or block scalar.
        for path in sorted((ROOT / '.github/workflows').glob('*.yml')):
            for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
                match = re.match(r'^\s+(?:-\s+)?run:\s+(.+)$', line)
                if match and not match[1].startswith(('"', "'", '|', '>')):
                    with self.subTest(workflow=path.name, line=number):
                        self.assertNotRegex(match[1], r':(?:\s|$)',
                                            'Quote the command or use run: |')

    def test_push_checks_run_native_windows_download_tests(self):
        text = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
        self.assertRegex(text, r"name: Native Windows download regression\s*\n\s*if: runner.os == 'Windows'\s*\n\s*run: python -m unittest discover -s tests/tooling -p test_online.py -v")

    def test_release_publication_builds_and_proves_public_installers(self):
        build = (ROOT / '.github/workflows/build.yml').read_text(encoding='utf-8')
        publish = (ROOT / '.github/workflows/release-assets.yml').read_text(encoding='utf-8')
        self.assertRegex(build, r"release:\s*\n\s*types:\s*\n\s*- published")
        self.assertIn('Require published release tag to match Cargo version', build)
        self.assertIn('Published release tag must be vX.Y.Z', build)
        self.assertIn('Release event ref does not match its published tag', build)
        self.assertIn('workflow_run:', publish)
        self.assertIn("--json isDraft --jq '.isDraft'", publish)
        self.assertIn("--json isPrerelease --jq '.isPrerelease'", publish)
        self.assertIn("--json publishedAt --jq '.publishedAt // \"\"'", publish)
        self.assertIn("Release $tag must be a published stable release, not a draft or prerelease", publish)
        self.assertIn("gh release upload", publish)
        self.assertNotIn('gh release upload "$RELEASE_TAG" .release-upload/* --clobber', publish)
        self.assertIn("Reuse exact existing installer assets without clobbering", publish)
        self.assertIn('gh release download "$RELEASE_TAG"', publish)
        self.assertIn("cmp -s", publish)
        self.assertIn("mode=resume", publish)
        self.assertIn(".release-publish/*", publish)
        self.assertIn("cleanup: ${{ steps.assets.outputs.cleanup }}", publish)
        self.assertIn("needs.publish.outputs.cleanup == 'true'", publish)
        self.assertIn("maris-release.tsv", publish)
        self.assertIn("public-install-smoke:", publish)
        self.assertIn("bash install.sh --version", publish)
        self.assertIn("& ./install.ps1 -Version", publish)
        public_smoke = publish.split('  public-install-smoke:', 1)[1].split('  remove-uninstallable-assets:', 1)[0]
        self.assertIn("if: needs.publish.outputs.mode != 'reuse'", public_smoke)
        self.assertIn('ref: ${{ github.event.workflow_run.head_sha }}', public_smoke)
        self.assertIn("if: needs.publish.outputs.mode == 'reuse'", public_smoke)
        self.assertIn('ref: ${{ github.event.repository.default_branch }}', public_smoke)
        self.assertIn("mode: ${{ steps.assets.outputs.mode }}", publish)
        self.assertIn("group: release-assets-", publish)
        self.assertIn("cancel-in-progress: false", publish)
        self.assertRegex(publish, r"permissions:\s*\n  contents: read\s*\n  actions: read")
        self.assertRegex(publish, r"publish:\s*\n(?:    .*\n)*?    permissions:\s*\n      contents: write\s*\n      actions: read")
        self.assertRegex(publish, r"public-install-smoke:\s*\n(?:    .*\n)*?    permissions:\s*\n      contents: read")
        self.assertRegex(publish, r"remove-uninstallable-assets:\s*\n(?:    .*\n)*?    permissions:\s*\n      contents: write")
        self.assertIn("remove-uninstallable-assets:", publish)
        self.assertIn("gh release delete-asset", publish)
        self.assertIn(
            "always() && needs.publish.outputs.publish == 'true' && needs.publish.outputs.cleanup == 'true' && (needs.publish.result != 'success' || needs.public-install-smoke.result != 'success')",
            publish,
        )
        self.assertNotIn("if: failure() && needs.publish.outputs.publish == 'true'", publish)
        for runner in ('macos-15', 'macos-15-intel', 'ubuntu-24.04', 'ubuntu-24.04-arm',
                       'windows-2025', 'windows-11-arm'):
            with self.subTest(runner=runner):
                self.assertIn(runner, publish)

    @unittest.skipUnless(BASH, 'Bash release identity execution required')
    def test_release_identity_gate_rejects_wrong_version_and_ref_before_native_matrix(self):
        text = (ROOT / '.github/workflows/build.yml').read_text(encoding='utf-8')
        block = re.search(
            r'      - name: Require published release tag to match Cargo version\n'
            r'(?:        .*\n)*?        run: \|\n((?:          .*\n?)+?)'
            r'      - name: Require release validation pushes to match current main\n',
            text,
        )
        self.assertIsNotNone(block)
        script = '\n'.join(line[10:] for line in block[1].splitlines())
        base = dict(os.environ, RELEASE_TAG='v0.1.0', GITHUB_REF='refs/tags/v0.1.0')
        result = subprocess.run(
            [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
            cwd=ROOT,
            env=base,
            capture_output=True,
            text=True,
            encoding='utf-8',
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for tag, ref in [('v0.1.1', 'refs/tags/v0.1.1'), ('release-0.1.0', 'refs/tags/release-0.1.0'),
                         ('v0.1.0', 'refs/tags/v0.1.1')]:
            with self.subTest(tag=tag, ref=ref):
                environment = dict(base, RELEASE_TAG=tag, GITHUB_REF=ref)
                result = subprocess.run(
                    [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                    cwd=ROOT,
                    env=environment,
                    capture_output=True,
                    text=True,
                    encoding='utf-8',
                    timeout=10,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)

    @unittest.skipUnless(BASH, 'Bash release idempotency execution required')
    def test_release_asset_publication_reuses_or_resumes_exact_assets_and_refuses_different_sets(self):
        text = (ROOT / '.github/workflows/release-assets.yml').read_text(encoding='utf-8')
        block = re.search(
            r'(?s)      - name: Reuse exact existing installer assets without clobbering\n'
            r'(?:        .*\n)*?        run: \|\n(.*?)'
            r'      - name: Upload installer-consumable assets to GitHub Release\n',
            text,
        )
        self.assertIsNotNone(block)
        script = '\n'.join(line[10:] for line in block[1].splitlines())
        names = [
            'Maris-1.2.3-macos-x86_64.tar.gz',
            'Maris-1.2.3-macos-arm64.tar.gz',
            'Maris-1.2.3-linux-x86_64.tar.gz',
            'Maris-1.2.3-linux-arm64.tar.gz',
            'Maris-1.2.3-windows-x86_64.zip',
            'Maris-1.2.3-windows-arm64.zip',
            'maris-release.tsv',
            'SHA256SUMS',
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            upload = root / '.release-upload'
            source = root / '.release-source'
            upload.mkdir()
            source.mkdir()
            for index, name in enumerate(names):
                payload = f'asset-{index}\n'
                (upload / name).write_text(payload, encoding='utf-8')
                (source / name).write_text(payload, encoding='utf-8')
            gh = root / 'gh'
            gh.write_text(
                '#!/bin/sh\n'
                'set -eu\n'
                'if [ "$1:$2" = release:view ]; then cat "$GH_FIXTURE/assets"; exit 0; fi\n'
                'if [ "$1:$2" = release:download ]; then\n'
                '  asset=""\n'
                '  dir=""\n'
                '  while [ "$#" -gt 0 ]; do\n'
                '    case "$1" in --pattern) asset="$2"; shift 2 ;; --dir) dir="$2"; shift 2 ;; *) shift ;; esac\n'
                '  done\n'
                '  [ "${FAIL_DOWNLOAD:-}" != "$asset" ] || exit 29\n'
                '  mkdir -p "$dir"\n'
                '  cp "$GH_FIXTURE/.release-source/$asset" "$dir/$asset"\n'
                '  exit 0\n'
                'fi\n'
                'exit 97\n',
                encoding='utf-8',
                newline='\n',
            )
            gh.chmod(0o755)
            assets = root / 'assets'
            output = root / 'github-output'

            def reset_workdirs():
                for name in ('.release-existing', '.release-publish'):
                    shutil.rmtree(root / name, ignore_errors=True)

            environment = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ['PATH'],
                GH_FIXTURE=str(root),
                GH_REPO='francis-du/maris',
                RELEASE_TAG='v1.2.3',
                GITHUB_OUTPUT=str(output),
            )

            assets.write_text('', encoding='utf-8')
            fresh = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=10,
            )
            self.assertEqual(fresh.returncode, 0, fresh.stdout + fresh.stderr)
            self.assertEqual(output.read_text().splitlines(), ['cleanup=true', 'mode=fresh'])
            self.assertEqual(sorted(path.name for path in (root / '.release-publish').iterdir()), sorted(names))

            output.unlink()
            reset_workdirs()
            assets.write_text('\n'.join(names) + '\n', encoding='utf-8')
            reuse = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=10,
            )
            self.assertEqual(reuse.returncode, 0, reuse.stdout + reuse.stderr)
            self.assertEqual(output.read_text().splitlines(), ['mode=reuse', 'cleanup=false'])
            self.assertFalse(any((root / '.release-publish').iterdir()))

            output.unlink()
            reset_workdirs()
            assets.write_text('\n'.join(names[:-1]) + '\n', encoding='utf-8')
            resume = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=10,
            )
            self.assertEqual(resume.returncode, 0, resume.stdout + resume.stderr)
            self.assertEqual(output.read_text().splitlines(), ['cleanup=true', 'mode=resume'])
            self.assertEqual([path.name for path in (root / '.release-publish').iterdir()], [names[-1]])

            output.unlink()
            reset_workdirs()
            assets.write_text('\n'.join(names) + '\n', encoding='utf-8')
            (source / names[0]).write_text('different\n', encoding='utf-8')
            different = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=10,
            )
            self.assertNotEqual(different.returncode, 0)
            self.assertEqual(output.read_text().splitlines(), ['cleanup=true'])

            # A transient inability to download one member of an otherwise complete
            # eight-asset set is not evidence that a previously valid publication is bad.
            # Fail the verification, but do not arm destructive cleanup.
            (source / names[0]).write_text('asset-0\n', encoding='utf-8')
            output.unlink()
            reset_workdirs()
            environment['FAIL_DOWNLOAD'] = names[0]
            transient = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=10,
            )
            self.assertEqual(transient.returncode, 29)
            self.assertFalse(output.exists())

    @unittest.skipUnless(BASH, 'Bash release cleanup execution required')
    def test_failed_public_install_cleanup_deletes_only_installer_assets_and_surfaces_errors(self):
        text = (ROOT / '.github/workflows/release-assets.yml').read_text(encoding='utf-8')
        block = re.search(
            r'      - name: Remove installer assets after any public installation failure\n'
            r'(?:        .*\n)*?        run: \|\n((?:          .*\n?)+)',
            text,
        )
        self.assertIsNotNone(block)
        script = '\n'.join(line[10:] for line in block[1].splitlines())
        expected = [
            'Maris-1.2.3-macos-x86_64.tar.gz',
            'Maris-1.2.3-macos-arm64.tar.gz',
            'Maris-1.2.3-linux-x86_64.tar.gz',
            'Maris-1.2.3-linux-arm64.tar.gz',
            'Maris-1.2.3-windows-x86_64.zip',
            'Maris-1.2.3-windows-arm64.zip',
            'maris-release.tsv',
            'SHA256SUMS',
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = root / 'assets'
            assets.write_text('\n'.join(expected + ['manual-release-notes.txt']) + '\n', encoding='utf-8')
            gh = root / 'gh'
            gh.write_text(
                '#!/bin/sh\n'
                'set -eu\n'
                'if [ "$1:$2" = release:view ]; then cat "$GH_FIXTURE/assets"; exit 0; fi\n'
                'if [ "$1:$2" = release:delete-asset ]; then\n'
                '  printf "%s\\n" "$4" >> "$GH_FIXTURE/deleted"\n'
                '  [ "${FAIL_ASSET:-}" != "$4" ] || exit 23\n'
                '  exit 0\n'
                'fi\n'
                'exit 97\n',
                encoding='utf-8',
                newline='\n',
            )
            gh.chmod(0o755)
            environment = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ['PATH'],
                GH_FIXTURE=str(root),
                GH_REPO='francis-du/maris',
                RELEASE_TAG='v1.2.3',
            )
            result = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                env=environment,
                capture_output=True,
                text=True,
                encoding='utf-8',
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((root / 'deleted').read_text().splitlines(), expected)

            (root / 'deleted').unlink()
            environment['FAIL_ASSET'] = expected[0]
            result = subprocess.run(
                [BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                env=environment,
                capture_output=True,
                text=True,
                encoding='utf-8',
                timeout=10,
            )
            self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
            self.assertEqual((root / 'deleted').read_text().splitlines(), [expected[0]])

    def test_developer_install_instructions_match_cargo(self):
        required = 'Rust ' + minimum_rust()
        paths = [ROOT / 'docs' / language / 'install.md'
                 for language in ('en', 'de', 'es', 'ja', 'zh-CN', 'zh-TW')]
        paths += [ROOT / 'install.ps1', ROOT / 'scripts/install_linux.sh',
                  ROOT / 'scripts/install_macos.sh', ROOT / 'docs/reference/commands.md']
        for path in paths:
            with self.subTest(path=str(path.relative_to(ROOT))):
                self.assertIn(required, path.read_text(encoding='utf-8'))


if __name__ == '__main__':
    unittest.main()
