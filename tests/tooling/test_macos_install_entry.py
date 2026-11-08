"""The root installer delegates to the macOS helper, which has no binary checksum path.

`--sha256` is documented as an offline Linux CLI option, so the entry point must
reject it on macOS itself instead of forwarding it and letting the helper fail
with an unrelated "Unknown option" message.
"""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALL = ROOT / 'install.sh'
HASH = '0' * 64


class MacosInstallEntry(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        uname = self.tools / 'uname'
        uname.write_text('#!' + sys.executable + '\nprint("Darwin")\n', encoding='utf-8')
        uname.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.tools) + os.pathsep + os.environ['PATH'])

    def run_installer(self, *args):
        return subprocess.run(['/bin/bash', str(INSTALL), '--from', str(self.root / 'payload'), *args],
                              env=self.env, capture_output=True, text=True, timeout=20)

    def test_sha256_is_rejected_on_macos_with_an_actionable_message(self):
        result = self.run_installer('--sha256', HASH)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--sha256 applies only to the offline Linux CLI payload', result.stderr)
        self.assertNotIn('Unknown option', result.stderr)

    def test_other_arguments_still_reach_the_macos_helper(self):
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('--sha256 applies only', result.stderr)


if __name__ == '__main__':
    unittest.main()
