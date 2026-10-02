"""Portable package validation uses documented ELF/PE headers, never executes a fixture."""
import importlib.util
import io
from pathlib import Path
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from portable_package import validate_binary, create_payload
SPEC = importlib.util.spec_from_file_location('maris_package', ROOT / 'scripts/package.py')
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


def fixture(system, arch):
    value = bytearray(256)
    if system == 'macos':
        value[:4] = b'\xcf\xfa\xed\xfe'
        struct.pack_into('<I', value, 4, 0x01000007 if arch == 'x86_64' else 0x0100000C)
        struct.pack_into('<I', value, 12, 2)
    elif system == 'linux':
        value[:7] = b'\x7fELF\x02\x01\x01'
        struct.pack_into('<H', value, 16, 3)
        struct.pack_into('<H', value, 18, 62 if arch == 'x86_64' else 183)
    else:
        value[:2] = b'MZ'
        struct.pack_into('<I', value, 60, 128)
        value[128:132] = b'PE\x00\x00'
        struct.pack_into('<H', value, 132, 0x8664 if arch == 'x86_64' else 0xAA64)
        struct.pack_into('<H', value, 150, 2)
        struct.pack_into('<H', value, 152, 0x20B)
    return value


class PortablePackages(unittest.TestCase):
    def test_valid_native_headers_and_wrong_architectures(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'fixture'
            for system in ['linux', 'windows']:
                for arch in ['x86_64', 'arm64']:
                    binary.write_bytes(fixture(system, arch))
                    validate_binary(binary, system, arch)
                    with self.assertRaises(ValueError):
                        validate_binary(binary, system, 'arm64' if arch == 'x86_64' else 'x86_64')
                    with self.assertRaises(ValueError):
                        validate_binary(binary, 'windows' if system == 'linux' else 'linux', arch)

    def test_truncated_pe_offsets_and_dlls_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'fixture'
            for data in [b'MZ', b'not executable', fixture('windows', 'x86_64')[:100]]:
                binary.write_bytes(data)
                with self.assertRaises(ValueError):
                    validate_binary(binary, 'windows', 'x86_64')
            dll = fixture('windows', 'x86_64')
            struct.pack_into('<H', dll, 150, 0x2002)
            binary.write_bytes(dll)
            with self.assertRaises(ValueError):
                validate_binary(binary, 'windows', 'x86_64')

    def test_payloads_include_correct_identity_and_notices_without_clobber(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            executable = root / 'fixture'
            for system in ['linux', 'windows']:
                executable.write_bytes(fixture(system, 'x86_64'))
                output = root / system / 'Maris'
                create_payload(ROOT, executable, output, system, 'x86_64', '0.1.0')
                self.assertEqual((output / '.maris-package').read_text(), f'maris-package-v1\n{system}\nx86_64\n0.1.0\n')
                self.assertTrue((output / 'resources/eqmac/LICENSE').is_file())
                self.assertTrue((output / 'resources/THIRD_PARTY.md').is_file())
                with self.assertRaises(ValueError):
                    create_payload(ROOT, executable, output, system, 'x86_64', '0.1.0')

    def test_tar_and_zip_preserve_payload_scope_and_do_not_contain_host_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            payload = root / 'Maris-fixture'
            payload.mkdir()
            (payload / 'notice.txt').write_text('fixture')
            for system, suffix in [('linux', '.tar.gz'), ('windows', '.zip')]:
                output = root / ('artifact' + suffix)
                package.archive_payload(payload, output, system)
                if system == 'linux':
                    with tarfile.open(output) as archive:
                        for info in archive.getmembers():
                            self.assertIn(info.name, ['Maris-fixture', 'Maris-fixture/notice.txt'])
                            self.assertEqual((info.uid, info.gid, info.uname, info.gname), (0, 0, '', ''))
                else:
                    with zipfile.ZipFile(output) as archive:
                        self.assertEqual(archive.namelist(), ['Maris-fixture/notice.txt'])

    def test_installer_sources_have_no_security_policy_or_service_mutation(self):
        linux = (ROOT / 'scripts/install_linux.sh').read_text()
        windows = (ROOT / 'install.ps1').read_text()
        for forbidden in ['Set-ExecutionPolicy', 'Unblock-File', 'Start-Service', 'Stop-Process', 'New-Service', 'Start-Process']:
            self.assertNotIn(forbidden, windows)
        for forbidden in ['systemctl ', 'sudo ', 'ldd ', 'curl ', 'killall ']:
            self.assertNotIn(forbidden, linux)
        self.assertIn('Get-AuthenticodeSignature', windows)
        self.assertNotIn('Get-FileHash', windows)
        self.assertIn('$sha.ComputeHash($stream)', windows)
        self.assertIn('File-Sha256 $file.FullName', windows)
        self.assertIn('File-Sha256 $archive', windows)
        self.assertIn('FileShare]::None', windows)
        self.assertIn('PE architecture', windows)
        self.assertIn('ELF machine', linux)
        self.assertIn('Linux) exec /bin/bash', (ROOT / 'install.sh').read_text())

    def test_build_workflow_has_native_jobs_for_every_platform(self):
        text = (ROOT / '.github/workflows/build.yml').read_text()
        for label in ['macos-15', 'macos-15-intel', 'ubuntu-24.04', 'ubuntu-24.04-arm', 'windows-2025', 'windows-11-arm']:
            self.assertIn(label, text)
        self.assertIn('scripts/package_smoke.py', text)
        self.assertNotIn('  push:', text)


if __name__ == '__main__':
    unittest.main()
