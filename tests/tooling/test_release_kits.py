"""CI catalog acceptance exercises actual tar/ZIP bytes, not self-reported metadata.

All payloads here are labeled non-executable fixtures. No release, network or audio.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from release_bundle import _normalize_pax_times, _pax_without_volatile_metadata, _tar_header_with_size, combine, inspect_kit, pack, release_output_directory, TARGETS
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
    def test_release_output_directory_rejects_non_directory_components(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'dist').write_text('not a directory', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'non-directory'):
                release_output_directory(root / 'dist/online-approved', root)

    @unittest.skipIf(sys.platform == 'win32', 'Directory symlink fixture requires native symlink support')
    def test_release_output_directory_rejects_symlink_escape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            outside = root / 'outside'
            outside.mkdir()
            (root / 'dist').symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'Linked release output'):
                release_output_directory(root / 'dist/online-approved', root)
            self.assertFalse((outside / 'online-approved').exists())

    def test_actual_six_target_archives_assemble_without_compilation_or_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_records(root)
            output = root / 'output/maris-release.tsv'
            self.assertEqual(combine(root, output), 'candidate')
            self.assertEqual(len(output.read_text().splitlines()), 10)
            self.assertIn('channel\tcandidate', output.read_text())

    def test_portable_release_pack_is_byte_reproducible_across_wall_time_and_source_mtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kit = root / 'Maris-1.2.3-fixture-arm64'
            (kit / 'Maris/bin').mkdir(parents=True)
            payload = kit / 'Maris/bin/maris'
            payload.write_bytes(b'deterministic portable package fixture')
            for system, suffix in [('linux', '.tar.gz'), ('windows', '.zip')]:
                first = root / f'first-{system}{suffix}'
                second = root / f'second-{system}{suffix}'
                pack(kit, first, system)
                time.sleep(1.05)
                os.utime(payload, (1_700_000_000, 1_700_000_123))
                pack(kit, second, system)
                self.assertEqual(
                    first.read_bytes(),
                    second.read_bytes(),
                    f'{system} release bytes depend on wall time or source mtime',
                )

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
            original_mtimes = {
                path.relative_to(kit): path.stat().st_mtime_ns
                for path in [kit, *sorted(kit.rglob('*'))]
            }
            archive = root / 'native.tar.gz'
            pack(kit, archive, 'macos')
            repeated = root / 'native-repeated.tar.gz'
            time.sleep(1.05)
            pack(kit, repeated, 'macos')
            self.assertEqual(
                original_mtimes,
                {
                    path.relative_to(kit): path.stat().st_mtime_ns
                    for path in [kit, *sorted(kit.rglob('*'))]
                },
                'macOS release packing modified input filesystem mtimes',
            )
            self.assertEqual(
                archive.read_bytes(),
                repeated.read_bytes(),
                'macOS native release bytes depend on wall time despite identical signed payload metadata',
            )
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

    @unittest.skipUnless(sys.platform == 'darwin', 'Native Apple archive metadata requires macOS')
    def test_native_mac_pack_is_independent_of_directory_insertion_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            stem = 'Maris-1.2.3-macos-arm64'
            kits = []
            for parent_name, names in [('forward', ['a.txt', 'b.txt', 'c.txt']),
                                       ('reverse', ['c.txt', 'b.txt', 'a.txt'])]:
                kit = root / parent_name / stem
                resources = kit / 'Maris.app/Contents/Resources'
                resources.mkdir(parents=True)
                for name in names:
                    (resources / name).write_text(f'payload-{name}\n', encoding='utf-8')
                (kit / '.maris-release').write_text('fixture\n', encoding='utf-8')
                subprocess.run(
                    ['/usr/bin/xattr', '-w', 'com.maris.archive-test', 'same metadata',
                     str(kit / 'Maris.app')],
                    check=True, capture_output=True, timeout=10,
                )
                kits.append(kit)
            for index, path in enumerate([kits[1], *sorted(kits[1].rglob('*'))]):
                stamp = 1_700_000_000 + index * 17
                os.utime(path, (stamp, stamp), follow_symlinks=False)
            first = root / 'forward.tar.gz'
            second = root / 'reverse.tar.gz'
            pack(kits[0], first, 'macos')
            pack(kits[1], second, 'macos')
            self.assertEqual(
                first.read_bytes(),
                second.read_bytes(),
                'macOS native archive depends on directory entry insertion order',
            )

    def test_pax_time_normalizer_preserves_non_time_records_and_rejects_malformed_lengths(self):
        def record(key, value):
            body = f' {key}={value}\n'.encode()
            for digits in range(1, 8):
                size = len(body) + digits
                if len(str(size)) == digits:
                    return str(size).encode() + body
            raise AssertionError('record length did not converge')

        payload = b''.join([
            record('ctime', '1791116074.416274307'),
            record('LIBARCHIVE.xattr.com.example', 'cGF5bG9hZA'),
            record('mtime', '1791116073'),
            record('uid', '501'),
            record('gid', '20'),
            record('uname', 'runner'),
            record('gname', 'staff'),
            record('path', 'Maris.app/Contents/MacOS/maris'),
        ])
        normalized = _pax_without_volatile_metadata(payload)
        for removed in (b'ctime=', b'mtime=', b'uid=', b'gid=', b'uname=', b'gname='):
            self.assertNotIn(removed, normalized)
        self.assertIn(b'LIBARCHIVE.xattr.com.example=cGF5bG9hZA', normalized)
        self.assertIn(b'path=Maris.app/Contents/MacOS/maris', normalized)

        for malformed in [
            b'',
            b'10 noequals\n',
            b'999 path=x\n',
            b'x path=x\n',
            b'8 path=x',
        ]:
            if malformed:
                with self.subTest(malformed=malformed), self.assertRaises(ValueError):
                    _pax_without_volatile_metadata(malformed)

    def test_tar_header_rewrite_normalizes_mtime_and_recomputes_checksum(self):
        header = bytearray(512)
        header[0:8] = b'fixture\0'
        header[108:116] = b'0000765\0'
        header[116:124] = b'0000020\0'
        header[124:136] = b'00000000012\0'
        header[136:148] = b'77777777777\0'
        header[148:156] = b'        '
        header[156:157] = b'x'
        header[257:263] = b'ustar\0'
        header[265:297] = b'runner-user\0' + b'\0' * 20
        header[297:329] = b'runner-group\0' + b'\0' * 19
        rewritten = _tar_header_with_size(bytes(header), 7)
        self.assertEqual(rewritten[108:116], b'0000000\0')
        self.assertEqual(rewritten[116:124], b'0000000\0')
        self.assertEqual(rewritten[124:136], b'00000000007\0')
        self.assertEqual(rewritten[136:148], b'00000000000\0')
        self.assertEqual(rewritten[265:297], b'\0' * 32)
        self.assertEqual(rewritten[297:329], b'\0' * 32)
        checksum = int(rewritten[148:154], 8)
        checkable = bytearray(rewritten)
        checkable[148:156] = b'        '
        self.assertEqual(checksum, sum(checkable))
        with self.assertRaises(ValueError):
            _tar_header_with_size(bytes(header), 8 ** 11)

    def test_failed_pax_normalization_preserves_original_bytes_and_cleans_temp(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'malformed.tar'
            original = b'not even one complete tar header'
            archive.write_bytes(original)
            with self.assertRaisesRegex(ValueError, 'Truncated native pax archive'):
                _normalize_pax_times(archive)
            self.assertEqual(archive.read_bytes(), original)
            self.assertFalse(list(root.glob('.maris-pax-*.tar')))

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
