"""Keep declared Rust requirements and native CI jobs in agreement."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def minimum_rust():
    # This project has one package. Keep the check dependency-free on Python 3.10.
    text = (ROOT / 'Cargo.toml').read_text(encoding='utf-8')
    versions = re.findall(r'^rust-version\s*=\s*"(\d+\.\d+)"\s*$', text, re.M)
    if len(versions) != 1:
        raise ValueError('Expected one explicit Rust requirement in Cargo.toml')
    return versions[0]


class ToolchainRequirements(unittest.TestCase):
    @unittest.skipUnless(shutil.which('bash'), 'Bash dependency gate execution required')
    def test_dependency_gates_reject_vulnerabilities_in_large_trees(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            cargo = temporary / 'cargo'
            cargo.write_text('#!/bin/sh\ncat "$MARIS_DEPENDENCY_TREE"\n', encoding='utf-8')
            cargo.chmod(0o755)
            tree = temporary / 'tree.txt'
            environment = dict(os.environ, PATH=str(temporary) + os.pathsep + os.environ.get('PATH', ''),
                               MARIS_DEPENDENCY_TREE=str(tree), RUNNER_OS='Regression fixture')
            for name in ('ci.yml', 'build.yml'):
                text = (ROOT / '.github/workflows' / name).read_text(encoding='utf-8')
                block = re.search(r'      - name: Reject vulnerable dependencies reachable on this native target\n'
                                  r'        shell: bash\n        run: \|\n((?:          .*\n)+)', text)
                self.assertIsNotNone(block, name)
                script = '\n'.join(line[10:] for line in block[1].splitlines())
                for dependency, rejected in [('glib v0.18.5', True), ('ringbuf v0.4.8', True),
                                             ('glib v0.21.5\nringbuf v0.5.2', False)]:
                    with self.subTest(workflow=name, dependency=dependency):
                        # Exceed pipe capacity so grep -q's early exit cannot hide SIGPIPE.
                        tree.write_text(dependency + '\n' + 'dependency v1.0.0\n' * 20_000, encoding='utf-8')
                        result = subprocess.run(['bash', '-c', script], cwd=temporary, env=environment,
                                                capture_output=True, text=True, timeout=10)
                        self.assertEqual(result.returncode, 1 if rejected else 0, result.stderr)

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
