"""Exercise production download/manifest/extraction paths with an isolated transport fixture.

No network requests, user installation, native executable execution or hardware claims.
Native local-install tests separately validate real Maris payloads and rollback.
"""
from pathlib import Path
import hashlib
import io
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from release_bundle import combine, TARGETS
from release_requirements import REQUIRED


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fixture(folder, system, arch, channel='stable', extra=None, *, cli=False):
    version = '1.2.3'; source = 'a' * 64; binary = b'offline fixture, not executable audio software\n'
    stem = f'Maris-{version}-{system}-{arch}'
    marker = '\n'.join(['maris-install-kit-v2' if cli else 'maris-install-kit-v1', version, system, arch, source, channel, sha(binary)] + (['cli'] if cli else [])) + '\n'
    binary_name = 'Maris.app/Contents/MacOS/maris' if system == 'macos' and not cli else ('Maris/bin/maris.exe' if system == 'windows' else 'Maris/bin/maris')
    helper = '''#!/bin/bash
set -eu
source=''; prefix=''
while [ "$#" -gt 0 ]; do
 case "$1" in --from) source=$2; shift 2;; --prefix) prefix=$2; shift 2;; --sha256) shift 2;; --yes) shift;; *) exit 23;; esac
done
[ -n "$source" ] && [ -n "$prefix" ]
physical=$(CDPATH= cd -- "$source" && pwd -P)
[ "$source" = "$physical" ] || exit 24
mkdir -p "$prefix"
printf 'verified fixture installer reached; no audio or real installation\\n' > "$prefix/fixture-receipt"
'''.encode()
    entries = {f'{stem}/.maris-release': marker.encode(), f'{stem}/{binary_name}': binary}
    if system == 'macos' and not cli:
        entries[f'{stem}/Maris.app/Contents/Info.plist'] = plistlib.dumps({'CFBundleShortVersionString': version})
    else:
        entries[f'{stem}/Maris/.maris-package'] = (f'maris-package-v2\n{system}\n{arch}\n{version}\ncli\n' if cli else f'maris-package-v1\n{system}\n{arch}\n{version}\n').encode()
    if system != 'windows':
        entries[f'{stem}/scripts/install_{"cli" if cli else system}.sh'] = helper
    else:
        entries[f'{stem}/install.ps1'] = b'# Isolated fixture installer, not a product payload\n'
    buffer = io.BytesIO()
    if system == 'windows':
        with zipfile.ZipFile(buffer, 'w', zipfile.ZIP_DEFLATED) as archive:
            for name, value in entries.items():
                archive.writestr(name, value)
            if extra:
                archive.writestr(extra[0], b'unsafe fixture')
    else:
        with tarfile.open(fileobj=buffer, mode='w:gz', format=tarfile.USTAR_FORMAT) as archive:
            for name, value in entries.items():
                info = tarfile.TarInfo(name); info.size = len(value); info.mode = 0o755 if name.endswith('.sh') or name.endswith('/maris') else 0o644
                archive.addfile(info, io.BytesIO(value))
            if extra:
                info = tarfile.TarInfo(extra[0]); info.mode = 0o644
                if extra[1] == 'link':
                    info.type = tarfile.SYMTYPE; info.linkname = '/outside-fixture'
                    archive.addfile(info)
                else:
                    info.size = 1; archive.addfile(info, io.BytesIO(b'x'))
    data = buffer.getvalue()
    (folder / 'archive').write_bytes(data)
    lines = ['maris-release-v2' if cli else 'maris-release-v1', f'version\t{version}', f'source_sha256\t{source}', f'channel\t{channel}']
    if cli:
        lines.append('interface\tcli')
    for target, machine in sorted(TARGETS):
        suffix = '.zip' if target == 'windows' else '.tar.gz'
        selected = (target, machine) == (system, arch)
        lines.append('\t'.join(['asset', target, machine, f'Maris-{version}-{target}-{machine}{suffix}', sha(data) if selected else 'b' * 64, str(len(data)) if selected else '1', sha(binary)]))
    manifest = '\n'.join(lines) + '\n'
    # The release wire format is LF-only, including on native Windows runners.
    (folder / 'manifest').write_bytes(manifest.encode('ascii'))
    return manifest, data


@unittest.skipIf(os.name == 'nt', 'Unix bootstrap; PowerShell production functions are checked separately')
class OnlineUnix(unittest.TestCase):
    def setUp(self):
        if os.getuid() == 0:
            self.skipTest('The production installer correctly refuses root')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.transport = self.root / 'transport'; self.transport.mkdir()
        self.tools = self.root / 'tools'; self.tools.mkdir()
        self.prefix = self.root / 'installation with spaces'
        self.script = self.root / 'standalone-install.sh'; shutil.copyfile(ROOT / 'install.sh', self.script)
        system = 'macos' if sys.platform == 'darwin' else 'linux'
        arch = 'arm64' if platform.machine().lower() in ('arm64','aarch64') else 'x86_64'
        self.system, self.arch = system, arch
        fixture(self.transport, system, arch)
        # Only transport is substituted. The actual installer parser, bounds, tar validation,
        # version pinning, checksum checks and installer delegation run unchanged.
        curl = self.tools / 'curl'
        curl.write_text('#!' + sys.executable + '\n' + '''import json, os, pathlib, sys
root = pathlib.Path(os.environ['MARIS_TRANSPORT_FIXTURE'])
with (root / 'requests').open('a') as file: file.write(json.dumps(sys.argv[1:]) + '\\n')
if (root / 'fail').exists(): raise SystemExit(22)
url = sys.argv[-1]
if not url.startswith('https://github.com/francis-du/maris/releases/'): raise SystemExit(70)
file = root / ('manifest' if url.endswith('/maris-release.tsv') else 'archive')
partial = root / 'partial-once'
if partial.exists() and file.name == 'archive':
    partial.unlink()
    sys.stdout.buffer.write(file.read_bytes()[:17])
    raise SystemExit(18)
sys.stdout.buffer.write(file.read_bytes())
''', encoding='utf-8')
        curl.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.tools) + os.pathsep + os.environ['PATH'], MARIS_TRANSPORT_FIXTURE=str(self.transport), TMPDIR=str(self.root))

    def run_installer(self, *args):
        return subprocess.run(['/bin/bash', str(self.script), '--prefix', str(self.prefix), *args],
                              env=self.env, capture_output=True, text=True, timeout=20)

    def unchanged(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse(self.prefix.exists())
        self.assertFalse(list(self.root.glob('maris-download.*')))

    @unittest.skipUnless(sys.platform == 'darwin', 'macOS compatibility symlink regression')
    def test_macos_online_cli_source_is_physical_before_local_installer(self):
        fixture(self.transport, self.system, self.arch, cli=True)
        result = self.run_installer('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.prefix / 'fixture-receipt').is_file())

    def test_standalone_latest_download_pins_tag_and_installs_without_cargo(self):
        result = self.run_installer('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.prefix / 'fixture-receipt').is_file())
        requests = [json.loads(line) for line in (self.transport / 'requests').read_text().splitlines()]
        self.assertTrue(requests[0][-1].endswith('/latest/download/maris-release.tsv'))
        self.assertIn('/download/v1.2.3/', requests[1][-1])
        for request in requests:
            self.assertIn('--proto', request); self.assertIn('--proto-redir', request)
            self.assertIn('=https', request); self.assertNotIn('--insecure', request)
        self.assertFalse(list(self.root.glob('maris-download.*')))
        self.assertFalse((self.root / 'target').exists())

    def test_partial_transfer_retries_into_a_fresh_file_not_an_appended_stream(self):
        (self.transport / 'partial-once').touch()
        result = self.run_installer('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.prefix / 'fixture-receipt').is_file())
        requests = [json.loads(line) for line in (self.transport / 'requests').read_text().splitlines()]
        package_requests = [request for request in requests if request[-1].endswith('.tar.gz')]
        self.assertEqual(len(package_requests), 2)
        self.assertEqual(package_requests[0][-1], package_requests[1][-1])
        self.assertFalse(list(self.root.glob('maris-download.*')))

    def test_case_colliding_archive_entries_are_rejected(self):
        stem = f'Maris-1.2.3-{self.system}-{self.arch}'
        fixture(self.transport, self.system, self.arch, extra=(stem + '/.MARIS-RELEASE', 'file'))
        result = self.run_installer('--yes')
        self.unchanged(result)
        self.assertIn('case-colliding', result.stderr)

    def test_dry_run_needs_no_checkout_and_makes_no_network_or_file_changes(self):
        result = self.run_installer('--version', 'v1.2.3', '--dry-run')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('online precompiled release', result.stdout)
        self.assertFalse((self.transport / 'requests').exists())
        self.assertFalse(self.prefix.exists())
        self.assertFalse(list(self.root.glob('maris-download.*')))

    def test_unpublished_or_failed_download_does_not_compile_or_install(self):
        (self.transport / 'fail').touch()
        result = self.run_installer('--yes')
        self.unchanged(result)
        self.assertIn('No', result.stderr)
        self.assertFalse((self.root / 'target').exists())

    def test_explicit_version_uses_pinned_manifest_and_rejects_mismatch(self):
        result = self.run_installer('--version', '1.2.4', '--yes')
        self.unchanged(result)
        request = json.loads((self.transport / 'requests').read_text().splitlines()[0])
        self.assertTrue(request[-1].endswith('/v1.2.4/maris-release.tsv'))

    def test_unsigned_flag_cannot_bypass_an_online_release(self):
        self.unchanged(self.run_installer('--allow-unsigned', '--yes'))
        self.assertFalse((self.transport / 'requests').exists())

    def test_partial_or_modified_archive_is_rejected_before_extraction(self):
        data = (self.transport / 'archive').read_bytes()
        for replacement in (data[:-1], bytes([data[0] ^ 1]) + data[1:]):
            (self.transport / 'archive').write_bytes(replacement)
            self.unchanged(self.run_installer('--yes'))

    def test_manifest_rejects_candidates_duplicates_bad_hashes_and_sizes(self):
        manifest = (self.transport / 'manifest').read_text()
        rows = manifest.splitlines()
        variants = [manifest.replace('channel\tstable', 'channel\tcandidate'),
                    '\n'.join(rows[:-1] + [rows[4]]) + '\n',
                    manifest.replace('source_sha256\t' + 'a' * 64, 'source_sha256\tinvalid'),
                    manifest + 'asset\tunexpected\n',
                    manifest.replace('\t1\t', '\t999999999\t')]
        for value in variants:
            (self.transport / 'manifest').write_text(value)
            self.unchanged(self.run_installer('--yes'))

    def test_traversal_links_and_duplicate_archive_entries_never_extract(self):
        stem = f'Maris-1.2.3-{self.system}-{self.arch}'
        for extra in [(stem + '/../../escape', 'file'), (stem + '/linked', 'link'),
                      (stem + '/.maris-release', 'file'), ('/absolute', 'file')]:
            fixture(self.transport, self.system, self.arch, extra=extra)
            self.unchanged(self.run_installer('--yes'))
            self.assertFalse((self.root / 'escape').exists())

    def test_invalid_version_or_conflicting_modes_fail_before_network(self):
        for args in [('--version','../../main'), ('--version','1.2.3','--build'), ('--build','--from','/tmp/fixture')]:
            self.unchanged(self.run_installer(*args))
        self.assertFalse((self.transport / 'requests').exists())


class ReleaseManifests(unittest.TestCase):
    def test_download_fixture_preserves_manifest_wire_bytes_on_windows(self):
        original_write_text = Path.write_text

        def windows_write_text(path, data, *args, **kwargs):
            # Reproduce Windows' default text-mode newline translation on any host.
            if kwargs.get('newline') is None:
                kwargs['newline'] = '\r\n'
            return original_write_text(path, data, *args, **kwargs)

        for arch in ('x86_64', 'arm64'):
            with self.subTest(architecture=arch), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with patch.object(Path, 'write_text', windows_write_text):
                    manifest, _ = fixture(root, 'windows', arch)
                actual = (root / 'manifest').read_bytes()
                self.assertEqual(actual, manifest.encode('ascii'))
                self.assertNotIn(b'\r', actual)

    def test_all_six_native_candidates_are_required_and_stay_candidate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            from test_release_kits import make_records
            make_records(root)
            output = root / 'maris-release.tsv'
            self.assertEqual(combine(root, output), 'candidate')
            self.assertIn('channel\tcandidate', output.read_text())
            with self.assertRaises(ValueError): combine(root, output)
            output.unlink()
            list(root.rglob('*.release.json'))[0].unlink()
            with self.assertRaises(ValueError): combine(root, output)
            self.assertFalse(output.exists())

    def test_current_unfinished_requirements_block_stable_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            from test_release_kits import make_records
            make_records(root, channel='stable')
            # Even structurally valid six-target archives do not waive real acceptance.
            with self.assertRaisesRegex(ValueError, 'Unfinished product requirements'):
                combine(root, root / 'maris-release.tsv')
            self.assertFalse((root / 'maris-release.tsv').exists())


@unittest.skipUnless(os.name == 'nt', 'Native PowerShell engine required')
class OnlineWindows(unittest.TestCase):
    def test_native_manifest_extraction_and_transport_functions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arch = 'arm64' if os.environ.get('PROCESSOR_ARCHITEW6432', os.environ.get('PROCESSOR_ARCHITECTURE', '')).lower() == 'arm64' else 'x86_64'
            fixture(root, 'windows', arch)
            result = subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-File',
                str(ROOT / 'tests/support/download_probe.ps1'),'-Fixture',str(root),'-Architecture',arch],
                capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('offline_download_paths_passed', result.stdout)


if __name__ == '__main__':
    unittest.main()
