import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import verify_rounds


class VerificationPathGuards(unittest.TestCase):
    def test_regular_verification_root_is_created_inside_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            verification = verify_rounds.verification_root(root)
            self.assertEqual(verification, root / ".maris-review" / "verification")
            self.assertTrue(verification.is_dir())

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_verification_root_rejects_linked_review_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root / "outside"
            outside.mkdir()
            (root / ".maris-review").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "Linked verification directory"):
                verify_rounds.verification_root(root)

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_resume_rejects_linked_run_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            verification = verify_rounds.verification_root(root)
            real = verification / "real-run"
            real.mkdir()
            linked = verification / "linked-run"
            linked.symlink_to(real, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "Resume must name"):
                verify_rounds.resume_directory(linked, verification)

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_checkpoint_rejects_linked_compiled_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real = root / "real-target"
            real.mkdir()
            (root / "target").symlink_to(real, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "compiled target directory"):
                verify_rounds.checkpoint_binaries({}, root)

    def test_checkpoint_rejects_non_directory_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "target").write_text("not a directory", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "compiled target directory"):
                verify_rounds.checkpoint_binaries({}, root)
