from contextlib import redirect_stdout
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tests" / "tooling"))

import release_bundle
from test_release_kits import make_records


class ReleaseCollectPathGuards(unittest.TestCase):
    def test_linked_directory_detects_windows_reparse_fallback(self):
        class FakePath:
            def __init__(self, attributes):
                self.attributes = attributes
            def is_symlink(self):
                return False
            def lstat(self):
                return type("Stat", (), {"st_file_attributes": self.attributes})()

        self.assertTrue(release_bundle.linked_directory(FakePath(0x400)))
        self.assertFalse(release_bundle.linked_directory(FakePath(0)))

    def run_collect(self, root, records, output):
        argv = ["release_bundle.py", "--collect", str(records), "--output", str(output)]
        with (
            patch.object(release_bundle, "ROOT", root),
            patch.object(sys, "argv", argv),
            patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}),
            redirect_stdout(io.StringIO()),
        ):
            release_bundle.main()

    def test_collect_accepts_real_input_and_output_inside_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            records = root / "records"
            records.mkdir()
            make_records(records)
            output = root / "manifest" / "maris-release.tsv"
            self.run_collect(root, records, output)
            self.assertTrue(output.is_file())

    def test_collect_rejects_input_outside_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            root = base / "checkout"
            root.mkdir()
            records = base / "outside-records"
            records.mkdir()
            make_records(records)
            output = root / "manifest" / "maris-release.tsv"
            with self.assertRaisesRegex(ValueError, "Release input directory must stay inside"):
                self.run_collect(root, records, output)
            self.assertFalse(output.exists())

    def test_collect_rejects_output_outside_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            root = base / "checkout"
            root.mkdir()
            records = root / "records"
            records.mkdir()
            make_records(records)
            output = base / "outside" / "maris-release.tsv"
            with self.assertRaisesRegex(ValueError, "Release output directory must stay inside"):
                self.run_collect(root, records, output)
            self.assertFalse(output.exists())

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_collect_rejects_linked_input_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            root = base / "checkout"
            root.mkdir()
            outside = base / "outside"
            outside.mkdir()
            make_records(outside)
            linked = root / "records"
            linked.symlink_to(outside, target_is_directory=True)
            output = root / "manifest" / "maris-release.tsv"
            with self.assertRaisesRegex(ValueError, "Linked release input directory"):
                self.run_collect(root, linked, output)
            self.assertFalse(output.exists())

    @unittest.skipIf(os.name == "nt", "POSIX symlink fixture complements reparse fallback")
    def test_release_input_path_rejects_linked_record_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real = root / "real"
            real.mkdir()
            record = make_records(real)[0]
            collection = root / "collection"
            collection.mkdir()
            linked_parent = collection / record.parent.name
            linked_parent.symlink_to(record.parent, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "release input parent"):
                release_bundle._release_input_path(
                    linked_parent / record.name, collection
                )
