import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import package_smoke


class PackageSmokePathGuards(unittest.TestCase):
    def test_link_guard_detects_windows_reparse_fallback(self):
        class FakePath:
            def __init__(self, attributes):
                self.attributes = attributes
            def is_symlink(self):
                return False
            def lstat(self):
                return type("Stat", (), {"st_file_attributes": self.attributes})()

        self.assertTrue(package_smoke.linked_directory(FakePath(0x400)))
        self.assertFalse(package_smoke.linked_directory(FakePath(0)))

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_package_input_directory_rejects_linked_components(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root.parent / (root.name + "-outside")
            outside.mkdir()
            try:
                real_dist = root / "real-dist"
                (real_dist / "packages").mkdir(parents=True)
                (root / "dist").symlink_to(real_dist, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, "Linked package-smoke input directory"):
                    package_smoke.package_input_directory(root)

                (root / "dist").unlink()
                (root / "dist").mkdir()
                (root / "dist" / "packages").symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, "Linked package-smoke input directory"):
                    package_smoke.package_input_directory(root)
            finally:
                outside.rmdir()

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_archive_and_checksum_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            packages = root / "dist" / "packages"
            packages.mkdir(parents=True)
            outside = root / "outside"
            outside.mkdir()

            external_archive = outside / "fixture.zip"
            external_archive.write_bytes(b"archive")
            archive = packages / "fixture.zip"
            archive.symlink_to(external_archive)
            with self.assertRaisesRegex(ValueError, "Linked or invalid package-smoke input file"):
                package_smoke.package_input_file(packages, archive)
            archive.unlink()

            external_checksum = outside / "fixture.sha256"
            external_checksum.write_text("0" * 64 + "  fixture.zip\n", encoding="utf-8")
            checksum = packages / "fixture.sha256"
            checksum.symlink_to(external_checksum)
            with self.assertRaisesRegex(ValueError, "Linked or invalid package-smoke input file"):
                package_smoke.package_input_file(packages, checksum)

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_review_directory_and_evidence_destination_reject_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root / "outside"
            outside.mkdir()

            (root / ".maris-review").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "Linked package-smoke review directory"):
                package_smoke.review_directory(root)
            (root / ".maris-review").unlink()

            review = package_smoke.review_directory(root)
            target = outside / "smoke.json"
            target.write_text("{}\n", encoding="utf-8")
            (review / "package-smoke.json").symlink_to(target)
            with self.assertRaisesRegex(ValueError, "Linked package-smoke evidence destination"):
                package_smoke.evidence_output(review)

    def test_missing_or_non_directory_package_root_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            with self.assertRaisesRegex(ValueError, "missing or invalid"):
                package_smoke.package_input_directory(root)
            (root / "dist").mkdir()
            (root / "dist" / "packages").write_text("not a directory", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "missing or invalid"):
                package_smoke.package_input_directory(root)
