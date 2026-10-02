"""The isolated Pulse protocol helper must not load ambient Python startup code."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.name == 'posix', 'Pulse subprocess fixtures run on Unix')
class PulseFixture(unittest.TestCase):
    def test_protocol_helper_ignores_pythonpath_startup_hooks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = root / 'pactl'
            shutil.copyfile(ROOT / 'tests/support/pulse_fixture.py', helper)
            helper.chmod(0o755)
            marker = root / 'ambient-hook-ran'
            (root / 'sitecustomize.py').write_text(
                f'from pathlib import Path\nPath({str(marker)!r}).touch()\n', encoding='utf-8')
            (root / 'fixture.json').write_text(json.dumps({'default': 'private-fixture'}), encoding='utf-8')
            result = subprocess.run(
                [str(helper), f'--server=unix:{root / "native"}', 'info'],
                env=dict(os.environ, PYTHONPATH=str(root)), stdin=subprocess.DEVNULL,
                capture_output=True, text=True, encoding='utf-8', timeout=2, check=True)
            self.assertEqual(json.loads(result.stdout), {'default_sink_name': 'private-fixture'})
            self.assertEqual(result.stderr, '')
            self.assertFalse(marker.exists(), 'Fixture executed an unrelated interpreter startup hook')
