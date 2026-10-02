"""Notice-byte and archive regressions. Synthetic fixtures are not release proof."""
from pathlib import Path
import copy
import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from dependency_notices import collect_standard_library, notice_entries, validate


def standard_library_fixture(directory):
    entries = []
    for name in ('COPYRIGHT-library.html', 'licenses/MIT.txt', 'licenses/Apache-2.0.txt'):
        path = directory / 'rust-stdlib' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(('Synthetic notice fixture: ' + name + '\r\n').encode())
        entries.append({'path': path.relative_to(directory).as_posix(),
                        'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    return {'release': '1.94.0', 'commit_hash': 'f' * 40, 'host': 'fixture-host', 'files': entries}


def notice_fixture(directory, native='fixture-native-target'):
    library = standard_library_fixture(directory)
    path = directory / 'cargo-fixture.txt'
    path.write_bytes(b'Synthetic Cargo notice fixture\n')
    return {'schema_version': 1, 'target': native, 'runtime_dependencies': [
        {'name': 'fixture', 'license': 'MIT', 'files': [
            {'path': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}]}],
        'bundled_assets': [], 'rust_standard_library': library}


class StandardLibraryNotices(unittest.TestCase):
    def test_collection_preserves_toolchain_texts_and_compiler_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'sysroot/share/doc/rust'
            source.mkdir(parents=True)
            fixture = standard_library_fixture(root / 'seed')
            for entry in fixture['files']:
                relative = Path(entry['path']).relative_to('rust-stdlib')
                destination = source / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((root / 'seed' / entry['path']).read_bytes())
            version = 'rustc fixture\nrelease: 1.94.0\ncommit-hash: ' + 'f' * 40 + '\nhost: fixture-host\n'
            results = [subprocess.CompletedProcess([], 0, version),
                       subprocess.CompletedProcess([], 0, str(root / 'sysroot') + '\n')]
            with patch('dependency_notices.subprocess.run', side_effect=results):
                result = collect_standard_library(root, root / 'output')
            self.assertEqual(result['release'], '1.94.0')
            self.assertEqual(result['commit_hash'], 'f' * 40)
            self.assertEqual(result['host'], 'fixture-host')
            for entry in result['files']:
                self.assertEqual((root / 'output' / entry['path']).read_bytes(),
                                 (root / 'seed' / entry['path']).read_bytes())
            (source / 'licenses/MIT.txt').unlink()
            with patch('dependency_notices.subprocess.run', side_effect=results):
                with self.assertRaisesRegex(ValueError, 'incomplete'):
                    collect_standard_library(root, root / 'incomplete')

    def test_missing_standard_library_or_identity_cannot_validate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record = notice_fixture(root)
            for key in ('rust_standard_library',):
                damaged = copy.deepcopy(record)
                del damaged[key]
                with self.assertRaises(ValueError):
                    notice_entries(damaged, 'fixture-native-target')
            for key in ('release', 'commit_hash', 'host', 'files'):
                damaged = copy.deepcopy(record)
                del damaged['rust_standard_library'][key]
                with self.assertRaises(ValueError):
                    notice_entries(damaged, 'fixture-native-target')
            record['rust_standard_library']['files'].pop()
            with self.assertRaises(ValueError):
                notice_entries(record, 'fixture-native-target')

    def test_changed_standard_library_bytes_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record = notice_fixture(root)
            (root / 'index.json').write_text(json.dumps(record))
            validate(root, 'fixture-native-target')
            notice = root / record['rust_standard_library']['files'][0]['path']
            notice.write_bytes(b'Changed notice after collection')
            with self.assertRaisesRegex(ValueError, 'modified'):
                validate(root, 'fixture-native-target')

    def test_actual_archive_requires_every_standard_library_notice(self):
        from release_bundle import inspect_kit
        from test_online import fixture
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _ = fixture(root, 'linux', 'x86_64', cli=True)
            asset = next(line.split('\t') for line in manifest.splitlines()
                         if line.startswith('asset\tlinux\tx86_64\t'))
            archive = root / 'archive'
            with tarfile.open(archive, 'r:gz') as source:
                entries = {item.name: source.extractfile(item).read() for item in source}
            notice_root = root / 'notices'
            notice_root.mkdir()
            record = notice_fixture(notice_root, 'x86_64-unknown-linux-gnu')
            index = (json.dumps(record) + '\n').encode()
            license_text = b'Synthetic project license status fixture\n'
            proof = {'notice_index_sha256': hashlib.sha256(index).hexdigest(),
                     'project_license_sha256': hashlib.sha256(license_text).hexdigest(),
                     'project_license_status': 'unspecified'}
            stem = 'Maris-1.2.3-linux-x86_64'
            prefix = stem + '/Maris/resources/notices/'
            entries[prefix + 'index.json'] = index
            entries[stem + '/CLI_VERIFICATION.json'] = json.dumps(proof).encode()
            entries[stem + '/Maris/resources/MARIS_LICENSE_STATUS.txt'] = license_text
            for entry in notice_entries(record, 'x86_64-unknown-linux-gnu'):
                entries[prefix + entry['path']] = (notice_root / entry['path']).read_bytes()
            item = {'platform': 'linux', 'architecture': 'x86_64', 'version': '1.2.3',
                    'source_sha256': 'a' * 64, 'channel': 'stable', 'binary_sha256': asset[6],
                    'interface': 'cli', 'native_gate': proof}

            def write_archive():
                with tarfile.open(archive, 'w:gz') as output:
                    for name, value in entries.items():
                        info = tarfile.TarInfo(name)
                        info.size = len(value)
                        output.addfile(info, io.BytesIO(value))

            write_archive()
            inspect_kit(archive, item)
            selected = prefix + 'rust-stdlib/COPYRIGHT-library.html'
            entries[selected] = b'Altered inside final archive'
            write_archive()
            with self.assertRaisesRegex(ValueError, 'notice is missing or modified'):
                inspect_kit(archive, item)
            del entries[selected]
            write_archive()
            with self.assertRaisesRegex(ValueError, 'notice is missing or modified'):
                inspect_kit(archive, item)


if __name__ == '__main__':
    unittest.main()
