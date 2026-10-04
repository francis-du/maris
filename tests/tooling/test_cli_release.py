"""Exercise real typed archive/installer paths; fixtures never claim native acceptance."""
from pathlib import Path
import hashlib
import json
import os
import struct
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from dependency_notices import validate as validate_notices
from portable_package import create_payload, validate_binary
from release_bundle import commit_release_inputs, inspect_kit
from test_online import fixture
import test_online
from test_portable import fixture as binary_fixture
from test_dependency_notices import standard_library_fixture


class CliContracts(unittest.TestCase):
    def test_all_six_cli_payloads_have_native_headers_and_a_distinct_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'non-executed-header-fixture'
            for system in ('macos', 'linux', 'windows'):
                for arch in ('x86_64', 'arm64'):
                    binary.write_bytes(binary_fixture(system, arch))
                    output = root / (system + '-' + arch)
                    create_payload(ROOT, binary, output, system, arch, '0.1.0', cli=True)
                    self.assertEqual((output / '.maris-package').read_text(encoding='utf-8').splitlines(),
                                     ['maris-package-v2', system, arch, '0.1.0', 'cli'])
                    validate_binary(binary, system, arch)
                    with self.assertRaises(ValueError):
                        validate_binary(binary, system, 'arm64' if arch == 'x86_64' else 'x86_64')
            malformed = binary_fixture('macos', 'arm64')
            struct.pack_into('<I', malformed, 12, 6)  # A dylib is not a CLI executable.
            binary.write_bytes(malformed)
            with self.assertRaises(ValueError):
                validate_binary(binary, 'macos', 'arm64')

    def test_actual_archive_identity_prevents_relabeling_gui_as_cli_or_the_reverse(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for system in ('macos', 'linux', 'windows'):
                for cli in (False, True):
                    manifest, _ = fixture(directory, system, 'arm64', cli=cli)
                    asset = next(line.split('\t') for line in manifest.splitlines() if line.startswith(f'asset\t{system}\tarm64\t'))
                    record = {'platform': system, 'architecture': 'arm64', 'version': '1.2.3',
                              'source_sha256': 'a' * 64, 'channel': 'stable', 'binary_sha256': asset[6],
                              'interface': 'cli' if cli else 'gui'}
                    inspect_kit(directory / 'archive', record)
                    if cli:
                        # Self-reported success cannot stand in for actual archived proof/notices.
                        record['native_gate'] = {'release_ready': True, 'notice_index_sha256': '0' * 64}
                        with self.assertRaises(ValueError):
                            inspect_kit(directory / 'archive', record)
                        record.pop('native_gate')
                    record['interface'] = 'gui' if cli else 'cli'
                    with self.assertRaises(ValueError):
                        inspect_kit(directory / 'archive', record)

    def test_notice_validation_reads_real_bytes_and_rejects_changed_or_escaping_notices(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            notice = directory / 'LICENSE.txt'
            notice.write_bytes(b'Actual fixture license text; not a legal approval.\n')
            entry = {'path': notice.name, 'sha256': hashlib.sha256(notice.read_bytes()).hexdigest()}
            record = {'schema_version': 1, 'target': 'fixture-native-target', 'runtime_dependencies':
                      [{'name': 'fixture', 'license': 'MIT', 'files': [entry]}], 'bundled_assets': [],
                      'rust_standard_library': standard_library_fixture(directory)}
            index = directory / 'index.json'
            index.write_text(json.dumps(record), encoding='utf-8')
            validate_notices(directory, 'fixture-native-target')
            notice.write_bytes(b'Changed after notice generation')
            with self.assertRaises(ValueError):
                validate_notices(directory, 'fixture-native-target')
            entry['path'] = '../outside'
            index.write_text(json.dumps(record), encoding='utf-8')
            with self.assertRaises(ValueError):
                validate_notices(directory, 'fixture-native-target')

    def test_release_input_commit_recovers_exact_orphan_and_refuses_different_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / 'packed.tar.gz'
            ready = root / 'ready.json'
            archive = root / 'published.tar.gz'
            record = root / 'published.json'
            packed.write_bytes(b'exact archive bytes')
            ready.write_bytes(b'{"exact":true}\n')
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()

            # Simulate a process crash after the archive hard-link but before the record.
            os.link(packed, archive)
            commit_release_inputs(packed, ready, archive, record, expected)
            self.assertEqual(archive.read_bytes(), packed.read_bytes())
            self.assertEqual(record.read_bytes(), ready.read_bytes())

            # A concurrent identical preparer is idempotent and does not rewrite either file.
            before_archive = archive.stat().st_ino
            before_record = record.stat().st_ino
            commit_release_inputs(packed, ready, archive, record, expected)
            self.assertEqual(archive.stat().st_ino, before_archive)
            self.assertEqual(record.stat().st_ino, before_record)

            # Existing data with the same names but different bytes must remain fail-closed.
            archive.unlink()
            archive.write_bytes(b'different archive')
            with self.assertRaisesRegex(ValueError, 'archive differs'):
                commit_release_inputs(packed, ready, archive, root / 'other.json', expected)

            archive.unlink()
            os.link(packed, archive)
            record.unlink()
            record.write_bytes(b'different record')
            with self.assertRaisesRegex(ValueError, 'record differs'):
                commit_release_inputs(packed, ready, archive, record, expected)

    def test_new_archive_is_rolled_back_when_record_conflicts_or_link_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / 'packed.tar.gz'
            ready = root / 'ready.json'
            archive = root / 'final.tar.gz'
            record = root / 'final.json'
            packed.write_bytes(b'exact archive bytes')
            ready.write_bytes(b'{"exact":true}\\n')
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()

            record.write_bytes(b'different record')
            with self.assertRaisesRegex(ValueError, 'record differs'):
                commit_release_inputs(packed, ready, archive, record, expected)
            self.assertFalse(archive.exists(), 'new archive survived a conflicting final record')
            self.assertEqual(record.read_bytes(), b'different record')

            record.unlink()
            real_link = os.link
            def fail_record_link(source, destination):
                if Path(destination) == record:
                    raise OSError('simulated record link failure')
                return real_link(source, destination)
            with patch('release_bundle.os.link', side_effect=fail_record_link):
                with self.assertRaisesRegex(OSError, 'simulated record link failure'):
                    commit_release_inputs(packed, ready, archive, record, expected)
            self.assertFalse(archive.exists(), 'new archive survived a record-link failure')
            self.assertFalse(record.exists())

            os.link(packed, archive)
            record.write_bytes(b'different record')
            with self.assertRaisesRegex(ValueError, 'record differs'):
                commit_release_inputs(packed, ready, archive, record, expected)
            self.assertEqual(archive.read_bytes(), packed.read_bytes())

    def test_identical_concurrent_release_input_commits_converge_to_one_exact_pair(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            packed = root / 'packed.tar.gz'
            ready = root / 'ready.json'
            archive = root / 'published.tar.gz'
            record = root / 'published.json'
            packed.write_bytes(b'concurrent exact archive')
            ready.write_bytes(b'{"concurrent":true}\n')
            expected = hashlib.sha256(packed.read_bytes()).hexdigest()
            barrier = threading.Barrier(2)
            failures = []

            def publish():
                try:
                    barrier.wait(timeout=5)
                    commit_release_inputs(packed, ready, archive, record, expected)
                except Exception as error:
                    failures.append(error)

            threads = [threading.Thread(target=publish) for _ in range(2)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(timeout=10)
            self.assertTrue(all(not thread.is_alive() for thread in threads))
            self.assertEqual(failures, [])
            self.assertEqual(archive.read_bytes(), packed.read_bytes())
            self.assertEqual(record.read_bytes(), ready.read_bytes())

    @unittest.skipUnless(os.name == 'nt', 'Native PowerShell production parser/extractor required')
    def test_native_windows_download_accepts_cli_contract_and_still_rejects_bad_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arch = 'arm64' if os.environ.get('PROCESSOR_ARCHITEW6432', os.environ.get('PROCESSOR_ARCHITECTURE', '')).lower() == 'arm64' else 'x86_64'
            fixture(directory, 'windows', arch, cli=True)
            result = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-File',
                str(ROOT / 'tests/support/download_probe.ps1'), '-Fixture', str(directory), '-Architecture', arch],
                capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('offline_download_paths_passed', result.stdout)


@unittest.skipIf(os.name == 'nt', 'Unix production bootstrap required')
class OnlineCliUnix(test_online.OnlineUnix):
    def setUp(self):
        super().setUp()
        fixture(self.transport, self.system, self.arch, cli=True)

    def test_cli_contract_cannot_silently_switch_to_the_gui_interface(self):
        manifest = (self.transport / 'manifest').read_text(encoding='utf-8')
        (self.transport / 'manifest').write_text(manifest.replace('interface\tcli', 'interface\tgui'), encoding='utf-8')
        self.unchanged(self.run_installer('--yes'))

    def test_cli_identity_is_required_in_the_actual_payload_not_only_manifest(self):
        manifest, _ = fixture(self.transport, self.system, self.arch, cli=False)
        rows = manifest.splitlines()
        rows[0] = 'maris-release-v2'
        rows.insert(4, 'interface\tcli')
        (self.transport / 'manifest').write_bytes(('\n'.join(rows) + '\n').encode('ascii'))
        self.unchanged(self.run_installer('--yes'))


if __name__ == '__main__':
    unittest.main()
