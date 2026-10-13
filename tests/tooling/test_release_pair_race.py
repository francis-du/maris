import hashlib
import os
from pathlib import Path
import sys
import tempfile
from unittest import TestCase, mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from release_bundle import commit_release_inputs


class ReleasePairRace(TestCase):
    def test_archive_replacement_between_validation_and_record_commit_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / "packed.tar.gz"
            ready = root / "ready.json"
            archive = root / "published.tar.gz"
            record = root / "published.json"

            packed.write_bytes(b"verified archive")
            ready.write_bytes(b'{"verified":true}\n')
            os.link(packed, archive)
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()

            real_link = os.link

            def replace_after_archive_validation(source, destination):
                if Path(destination) == record:
                    replacement = root / "replacement"
                    replacement.write_bytes(b"attacker replacement")
                    os.replace(replacement, archive)
                return real_link(source, destination)

            with mock.patch("release_bundle.os.link", side_effect=replace_after_archive_validation):
                with self.assertRaisesRegex(
                    ValueError, "Release archive/record pair changed while being committed"
                ):
                    commit_release_inputs(packed, ready, archive, record, expected)

            self.assertEqual(archive.read_bytes(), b"attacker replacement")
            self.assertFalse(
                record.exists(),
                "record created by this attempt survived after final pair verification failed",
            )

    def test_record_replacement_after_link_is_not_deleted_as_if_it_were_ours(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / "packed.tar.gz"
            ready = root / "ready.json"
            archive = root / "published.tar.gz"
            record = root / "published.json"

            packed.write_bytes(b"verified archive")
            ready.write_bytes(b'{"verified":true}\n')
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()
            real_link = os.link

            def replace_record_after_link(source, destination):
                real_link(source, destination)
                if Path(destination) == record:
                    replacement = root / "replacement-record"
                    replacement.write_bytes(b'{"other":true}\n')
                    os.replace(replacement, record)

            with mock.patch("release_bundle.os.link", side_effect=replace_record_after_link):
                with self.assertRaisesRegex(
                    ValueError, "Release archive/record pair changed while being committed"
                ):
                    commit_release_inputs(packed, ready, archive, record, expected)

            self.assertFalse(
                archive.exists(),
                "archive created by this attempt survived a mismatched final record",
            )
            self.assertEqual(
                record.read_bytes(),
                b'{"other":true}\n',
                "cleanup deleted a concurrently replaced record it did not create",
            )
