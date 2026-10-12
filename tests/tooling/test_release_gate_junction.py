"""Release-gate linked-path regressions without publication or network access."""
from pathlib import Path
import os
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import release_gate


class ReleaseGateJunctions(unittest.TestCase):
    def test_windows_reparse_fallback_is_treated_as_linked(self):
        class FakePath:
            def __init__(self, attributes):
                self.attributes = attributes

            def is_symlink(self):
                return False

            def lstat(self):
                return type("Stat", (), {"st_file_attributes": self.attributes})()

        self.assertTrue(release_gate.linked_directory(FakePath(0x400)))
        self.assertFalse(release_gate.linked_directory(FakePath(0)))

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture is covered by the reparse fallback")
    def test_bundle_digest_rejects_linked_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / "Maris.app"
            bundle.mkdir()
            target = root / "outside.txt"
            target.write_text("outside", encoding="utf-8")
            (bundle / "linked.txt").symlink_to(target)
            with self.assertRaisesRegex(ValueError, "linked or junction"):
                release_gate.bundle_digest(bundle)


if __name__ == "__main__":
    unittest.main()
