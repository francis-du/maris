"""CLI release path ownership without publishing or running native packages."""
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from cli_release import release_destination, release_evidence_file, release_package_inputs


class CliReleasePathGuards(unittest.TestCase):
    @unittest.skipIf(os.name == 'nt', 'Directory symlink fixture requires native symlink support')
    def test_cli_release_locations_reject_linked_input_and_output_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root.parent / (root.name + '-outside')
            outside.mkdir()
            try:
                (root / 'dist').mkdir()
                (root / 'dist/packages').symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, 'Linked release input directory'):
                    release_package_inputs(root, '*.zip')
                (root / 'dist/packages').unlink()
                (root / 'dist/packages').mkdir()
                package = root / 'dist/packages/fixture.zip'
                target = outside / 'fixture.zip'
                target.write_bytes(b'outside package')
                package.symlink_to(target)
                with self.assertRaisesRegex(ValueError, 'Linked release input'):
                    release_package_inputs(root, '*.zip')
                package.unlink()
                (root / 'dist/online-approved').symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, 'Linked release output'):
                    release_destination(root)
            finally:
                for child in outside.iterdir():
                    child.unlink()
                outside.rmdir()

    def test_missing_cli_package_does_not_create_release_output_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            self.assertEqual(release_package_inputs(root, '*.zip'), [])
            self.assertFalse((root / 'dist/online-approved').exists())

    @unittest.skipIf(os.name == 'nt', 'Directory symlink fixture requires native symlink support')
    def test_cli_release_evidence_rejects_direct_and_parent_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            evidence = root / 'real/report.json'
            evidence.parent.mkdir()
            evidence.write_text('{}\n', encoding='utf-8')
            direct = root / 'direct-report.json'
            direct.symlink_to(evidence)
            with self.assertRaisesRegex(ValueError, 'Linked release input'):
                release_evidence_file(root, direct)
            linked_parent = root / 'linked'
            linked_parent.symlink_to(evidence.parent, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'input parent'):
                release_evidence_file(root, linked_parent / evidence.name)
