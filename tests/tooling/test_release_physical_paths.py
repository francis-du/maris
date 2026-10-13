from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tests" / "tooling"))

import release_bundle
from release_bundle import (
    _release_input_path,
    combine,
    linked_directory,
    release_input_directory,
    release_output_directory,
)
from test_online import fixture


def make_records(root: Path) -> list[Path]:
    records = []
    for system, arch in sorted(release_bundle.TARGETS):
        directory = root / f"{system}-{arch}"
        directory.mkdir()
        manifest, data = fixture(directory, system, arch, channel="candidate")
        selected = next(
            line.split("\t")
            for line in manifest.splitlines()
            if line.startswith(f"asset\t{system}\t{arch}\t")
        )
        name = selected[3]
        (directory / name).write_bytes(data)
        record = {
            "schema_version": 1,
            "version": "1.2.3",
            "platform": system,
            "architecture": arch,
            "archive": name,
            "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
            "binary_sha256": selected[6],
            "source_sha256": "a" * 64,
            "channel": "candidate",
            "native_gate": None,
            "ci_run": "1234",
            "ci_commit": "c" * 40,
            "ci_repository": "francis-du/maris",
        }
        path = directory / f"Maris-1.2.3-{system}-{arch}.release.json"
        path.write_text(json.dumps(record), encoding="utf-8")
        records.append(path)
    return records


class ReleasePhysicalPaths(unittest.TestCase):
    def test_linked_directory_detects_windows_reparse_fallback_without_windows(self):
        class FakePath:
            def __init__(self, attributes):
                self.attributes = attributes

            def is_symlink(self):
                return False

            def lstat(self):
                return type("Stat", (), {"st_file_attributes": self.attributes})()

        self.assertTrue(linked_directory(FakePath(0x400)))
        self.assertFalse(linked_directory(FakePath(0)))

    def test_ambiguous_output_path_is_rejected_before_outside_directory_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root.parent / (root.name + "-outside")
            if outside.exists():
                shutil.rmtree(outside)
            target = root / "nested" / ".." / ".." / outside.name
            with self.assertRaisesRegex(ValueError, "ambiguous path"):
                release_output_directory(target, root)
            self.assertFalse(outside.exists())

    def test_release_input_directory_rejects_non_directory_and_ambiguous_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "records").write_text("not a directory", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "non-directory"):
                release_input_directory(root / "records" / "target", root)
            with self.assertRaisesRegex(ValueError, "ambiguous path"):
                release_input_directory(root / "child" / "..", root)

    @unittest.skipIf(os.name == "nt", "POSIX link fixture complements reparse test")
    def test_combine_rejects_linked_collection_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real = root / "real-records"
            real.mkdir()
            make_records(real)
            linked = root / "linked-records"
            linked.symlink_to(real, target_is_directory=True)
            output = root / "manifest.tsv"
            with self.assertRaisesRegex(ValueError, "release input directory"):
                combine(linked, output)
            self.assertFalse(output.exists())

    @unittest.skipIf(os.name == "nt", "POSIX link fixture complements reparse test")
    def test_release_input_path_rejects_linked_record_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real = root / "real"
            real.mkdir()
            record = make_records(real)[-1]
            collection = root / "collection"
            collection.mkdir()
            linked_parent = collection / record.parent.name
            linked_parent.symlink_to(record.parent, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "input parent"):
                _release_input_path(linked_parent / record.name, collection)

    def test_collect_cli_anchors_real_input_and_output_below_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            records = root / "records"
            records.mkdir()
            make_records(records)
            output = root / "manifest" / "maris-release.tsv"
            argv = [
                "release_bundle.py",
                "--collect",
                str(records),
                "--output",
                str(output),
            ]
            with (
                patch.object(release_bundle, "ROOT", root),
                patch.object(sys, "argv", argv),
                patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}),
                redirect_stdout(io.StringIO()),
            ):
                release_bundle.main()
            self.assertTrue(output.is_file())

    @unittest.skipIf(os.name == "nt", "POSIX link fixture complements reparse test")
    def test_collect_cli_rejects_linked_checkout_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root.parent / (root.name + "-records")
            outside.mkdir()
            try:
                make_records(outside)
                linked = root / "linked"
                linked.symlink_to(outside, target_is_directory=True)
                output = root / "manifest" / "maris-release.tsv"
                argv = [
                    "release_bundle.py",
                    "--collect",
                    str(linked),
                    "--output",
                    str(output),
                ]
                with (
                    patch.object(release_bundle, "ROOT", root),
                    patch.object(sys, "argv", argv),
                    redirect_stdout(io.StringIO()),
                ):
                    with self.assertRaisesRegex(
                        ValueError, "Linked release input directory"
                    ):
                        release_bundle.main()
                self.assertFalse(output.exists())
            finally:
                shutil.rmtree(outside)
