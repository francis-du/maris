import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import release_gate


class ReleaseGatePathGuards(unittest.TestCase):
    def test_reparse_guard_detects_windows_junction_fallback(self):
        class FakePath:
            def __init__(self, attributes):
                self.attributes = attributes
            def is_symlink(self):
                return False
            def lstat(self):
                return type("Stat", (), {"st_file_attributes": self.attributes})()

        self.assertTrue(release_gate.linked_directory(FakePath(0x400)))
        self.assertFalse(release_gate.linked_directory(FakePath(0)))

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_evidence_path_rejects_linked_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real = root / "real"
            real.mkdir()
            evidence = real / "report.json"
            evidence.write_text("{}\n", encoding="utf-8")
            linked = root / "linked"
            linked.symlink_to(real, target_is_directory=True)

            with self.assertRaisesRegex(ValueError, "Linked or junction evidence"):
                release_gate.within_root(linked / evidence.name, root)

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
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

    def test_regular_bundle_digest_remains_stable(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "Maris.app"
            bundle.mkdir()
            payload = bundle / "payload"
            payload.write_bytes(b"stable")
            first = release_gate.bundle_digest(bundle)
            second = release_gate.bundle_digest(bundle)
            self.assertEqual(first, second)
