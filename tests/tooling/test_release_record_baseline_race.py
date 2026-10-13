import json
import os
from pathlib import Path
import sys
import tempfile
from unittest import TestCase, mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tests" / "tooling"))

import release_bundle
from test_release_kits import make_records


class ReleaseRecordBaselineRace(TestCase):
    def test_current_record_replacement_after_parse_blocks_manifest_commit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            records = make_records(root)
            victim_record = records[0]
            output = root / "manifest.tsv"

            original = release_bundle.inspect_kit
            calls = 0

            def inspect_then_replace(archive, item):
                nonlocal calls
                original(archive, item)
                calls += 1
                if calls == 1:
                    replacement_value = json.loads(
                        victim_record.read_text(encoding="utf-8")
                    )
                    replacement_value["source_sha256"] = "b" * 64
                    replacement = root / "replacement-record"
                    replacement.write_text(
                        json.dumps(replacement_value), encoding="utf-8"
                    )
                    os.replace(replacement, victim_record)

            with mock.patch(
                "release_bundle.inspect_kit", side_effect=inspect_then_replace
            ):
                with self.assertRaisesRegex(
                    ValueError, "Release inputs changed while building the manifest"
                ):
                    release_bundle.combine(root, output)

            self.assertFalse(output.exists())
