"""CI catalog acceptance exercises actual tar/ZIP bytes, not self-reported metadata.

All payloads here are labeled non-executable fixtures. No release, network or audio.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from release_bundle import combine, inspect_kit, pack, TARGETS
from package_smoke import extract
from test_online import fixture


def make_records(root: Path, *, ci_run='1234', ci_commit='c' * 40, channel='candidate') -> list[Path]:
    records = []
    for system, arch in sorted(TARGETS):
        directory = root / f'{system}-{arch}'
        directory.mkdir()
        manifest, data = fixture(directory, system, arch, channel=channel)
        selected = next(line.split('\t') for line in manifest.splitlines()
                        if line.startswith(f'asset\t{system}\t{arch}\t'))
        name = selected[3]
        (directory / name).write_bytes(data)
        record = dict(schema_version=1, version='1.2.3', platform=system, architecture=arch,
                      archive=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                      binary_sha256=selected[6], source_sha256='a' * 64, channel=channel,
                      native_gate=None, ci_run=ci_run, ci_commit=ci_commit, ci_repository='francis-du/maris')
        path = directory / f'Maris-1.2.3-{system}-{arch}.release.json'
        path.write_text(json.dumps(record), encoding='utf-8')
        records.append(path)
    return records


class ReleaseKits(unittest.TestCase):
    def test_actual_six_target_archives_assemble_without_compilation_or_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_records(root)
            output = root / 'output/maris-release.tsv'
            self.assertEqual(combine(root, output), 'candidate')
            self.assertEqual(len(output.read_text().splitlines()), 10)
            self.assertIn('channel\tcandidate', output.read_text())

    @unittest.skipUnless(sys.platform == 'darwin', 'Native Apple archive metadata requires macOS')
    def test_native_mac_kit_keeps_payload_metadata_inside_the_package_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            records = make_records(root)
            record = json.loads(next(path for path in records if '-macos-arm64.' in path.name).read_text())
            original = root / 'macos-arm64' / record['archive']
            stem = record['archive'][:-len('.tar.gz')]
            # Known generated tar fixture only. It is not an untrusted extraction shortcut.
            extract(original, root / 'source', 'linux', stem)
            kit = root / 'source' / stem
            for path, value in [(kit, 'generated wrapper metadata'),
                                (kit / 'Maris.app', 'payload metadata must survive')]:
                subprocess.run(['/usr/bin/xattr', '-w', 'com.maris.archive-test', value, str(path)],
                               check=True, capture_output=True, timeout=10)
            archive = root / 'native.tar.gz'
            pack(kit, archive, 'macos')
            with tarfile.open(archive, 'r:gz') as source:
                self.assertTrue(all(entry.name.startswith(stem + '/') for entry in source))
            inspect_kit(archive, record)
            restored = root / 'restored'; restored.mkdir()
            subprocess.run(['/usr/bin/tar', '--mac-metadata', '-xpf', str(archive), '-C', str(restored)],
                           check=True, capture_output=True, timeout=30)
            metadata = subprocess.run(['/usr/bin/xattr', '-p', 'com.maris.archive-test',
                                       str(restored / stem / 'Maris.app')],
                                      check=True, capture_output=True, timeout=10).stdout.rstrip(b'\n')
            self.assertEqual(metadata, b'payload metadata must survive')

    def test_concurrent_collectors_cannot_both_publish_the_same_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_records(root)
            output = root / 'output/maris-release.tsv'
            barrier = threading.Barrier(2)
            original_exists = Path.exists

            def synchronized_exists(path):
                if path == output:
                    barrier.wait(timeout=10)
                    return False
                return original_exists(path)

            results = []
            failures = []

            def collect():
                try:
                    results.append(combine(root, output))
                except ValueError as error:
                    failures.append(str(error))

            with patch.object(Path, 'exists', synchronized_exists):
                threads = [threading.Thread(target=collect) for _ in range(2)]
                for thread in threads:
                    thread.start()
                for thread in threads:
                    thread.join(timeout=30)
                self.assertTrue(all(not thread.is_alive() for thread in threads))

            self.assertEqual(results, ['candidate'])
            self.assertEqual(failures, ['Manifest destination already exists'])
            self.assertEqual(len(output.read_text().splitlines()), 10)
            self.assertFalse(list(output.parent.glob('.maris-release.*')))

    def test_manifest_publish_failure_leaves_no_partial_output_or_temp_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_records(root)
            output = root / 'output/maris-release.tsv'
            with patch('release_bundle.os.link', side_effect=OSError('simulated publish failure')):
                with self.assertRaisesRegex(OSError, 'simulated publish failure'):
                    combine(root, output)
            self.assertFalse(output.exists())
            self.assertFalse(list(output.parent.glob('.maris-release.*')))

    def test_recomputed_archive_hash_does_not_make_arbitrary_bytes_a_ci_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record_path = make_records(root)[0]
            record = json.loads(record_path.read_text())
            data = b'this is not a tar archive'
            (record_path.parent / record['archive']).write_bytes(data)
            record.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
            record_path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                combine(root, root / 'maris-release.tsv')
            self.assertFalse((root / 'maris-release.tsv').exists())

    def test_record_cannot_claim_a_different_binary_from_the_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record_path = make_records(root)[0]
            record = json.loads(record_path.read_text())
            record['binary_sha256'] = '0' * 64
            record_path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                combine(root, root / 'maris-release.tsv')
            self.assertFalse((root / 'maris-release.tsv').exists())

    def test_identical_source_claims_must_match_the_embedded_package_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for record_path in make_records(root):
                record = json.loads(record_path.read_text())
                record['source_sha256'] = 'd' * 64
                record_path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                combine(root, root / 'maris-release.tsv')

    def test_mixed_ci_runs_and_commits_are_not_one_build(self):
        for changed in ({'ci_run': '5678'}, {'ci_commit': 'd' * 40}):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                path = make_records(root)[0]
                record = json.loads(path.read_text()); record.update(changed)
                path.write_text(json.dumps(record))
                with self.assertRaises(ValueError):
                    combine(root, root / 'maris-release.tsv')

    def test_expected_ci_run_cannot_be_satisfied_by_a_different_successful_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_records(root)
            for expected in [
                {'run': '5678', 'commit': 'c' * 40, 'repository': 'francis-du/maris'},
                {'run': '1234', 'commit': 'd' * 40, 'repository': 'francis-du/maris'},
                {'run': '1234', 'commit': 'c' * 40, 'repository': 'other/maris'},
            ]:
                with self.subTest(expected=expected), self.assertRaisesRegex(ValueError, 'exact CI run'):
                    combine(root, root / 'maris-release.tsv', expected_ci=expected)
                self.assertFalse((root / 'maris-release.tsv').exists())

    def test_archive_case_aliases_are_rejected_before_catalog_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = make_records(root)[0]
            record = json.loads(path.read_text())
            stem = f"Maris-{record['version']}-{record['platform']}-{record['architecture']}"
            _, data = fixture(path.parent, record['platform'], record['architecture'], 'candidate',
                              extra=(stem + '/.MARIS-RELEASE', 'file'))
            (path.parent / record['archive']).write_bytes(data)
            record.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
            path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                combine(root, root / 'maris-release.tsv')


if __name__ == '__main__':
    unittest.main()
