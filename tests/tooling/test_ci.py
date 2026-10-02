"""Keep declared Rust requirements and native CI jobs in agreement."""
from pathlib import Path
import re
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
