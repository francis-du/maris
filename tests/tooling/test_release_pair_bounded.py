import hashlib
import os
from pathlib import Path
import sys
import tempfile
from unittest import TestCase, mock

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

    def test_oversized_record_conflict_rolls_back_new_archive_without_reading_payload(self):
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
                with self.assertRaisesRegex(ValueError, "record differs"):
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
            real_linked = release_bundle.linked_directory

            def linked(path):
                return Path(path) == record or real_linked(path)

            with mock.patch("release_bundle.linked_directory", side_effect=linked):
                with self.assertRaisesRegex(ValueError, "record differs"):
                    commit_release_inputs(packed, ready, archive, record, expected)

            self.assertFalse(archive.exists())
            self.assertEqual(record.read_bytes(), ready.read_bytes())
