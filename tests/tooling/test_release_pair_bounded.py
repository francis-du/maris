import hashlib
import os
from pathlib import Path
import sys
import tempfile
from unittest import TestCase, mock
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import release_bundle
from release_bundle import commit_release_inputs


class ReleasePairBoundedComparison(TestCase):
    def test_exact_existing_pair_never_uses_unbounded_path_read_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / "packed.tar.gz"
            ready = root / "ready.json"
            archive = root / "published.tar.gz"
            record = root / "published.release.json"
            packed.write_bytes(b"exact archive")
            ready.write_bytes(b'{"exact":true}\n')
            os.link(packed, archive)
            os.link(ready, record)
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()

            with mock.patch.object(
                Path,
                "read_bytes",
                side_effect=AssertionError("unbounded read_bytes used"),
            ):
                commit_release_inputs(packed, ready, archive, record, expected)

            self.assertTrue(archive.samefile(packed))
            self.assertTrue(record.samefile(ready))

    def test_oversized_record_conflict_is_rejected_before_archive_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / "packed.tar.gz"
            ready = root / "ready.json"
            archive = root / "published.tar.gz"
            record = root / "published.release.json"
            packed.write_bytes(b"exact archive")
            ready.write_bytes(b'{"exact":true}\n')
            with record.open("wb") as output:
                output.truncate(32769)
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()

            with mock.patch.object(
                Path,
                "read_bytes",
                side_effect=AssertionError("oversized record payload was read"),
            ):
                with self.assertRaisesRegex(ValueError, "oversized"):
                    commit_release_inputs(packed, ready, archive, record, expected)

            self.assertFalse(archive.exists())
            self.assertEqual(record.stat().st_size, 32769)

    def test_reparse_like_final_record_is_rejected_and_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / "packed.tar.gz"
            ready = root / "ready.json"
            archive = root / "published.tar.gz"
            record = root / "published.release.json"
            packed.write_bytes(b"exact archive")
            ready.write_bytes(b'{"exact":true}\n')
            record.write_bytes(ready.read_bytes())
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()
            real_lstat = Path.lstat

            def reparse_lstat(path, *args, **kwargs):
                info = real_lstat(path, *args, **kwargs)
                if Path(path) == record:
                    fields = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns")
                    return SimpleNamespace(
                        **{field: getattr(info, field) for field in fields},
                        st_file_attributes=0x400,
                    )
                return info

            with mock.patch.object(Path, "lstat", reparse_lstat):
                with self.assertRaisesRegex(ValueError, "Missing, linked"):
                    commit_release_inputs(packed, ready, archive, record, expected)

            self.assertFalse(archive.exists())
            self.assertEqual(record.read_bytes(), ready.read_bytes())
