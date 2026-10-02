#!/usr/bin/env python3
"""Verify a current native archive, then dry-run/install inside isolated temporary directories."""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from portable_package import host_target
from source_audit import ROOT, source_digest


def safe_name(name: str, stem: str) -> bool:
    path = PurePosixPath(name)
    return (not path.is_absolute() and bool(path.parts) and path.parts[0] in (stem, '__MACOSX')
            and '..' not in path.parts and '\\' not in name and '\x00' not in name)


def extract(archive: Path, directory: Path, system: str, stem: str) -> None:
    total = 0
    if system == 'linux':
        with tarfile.open(archive, 'r:gz') as source:
            entries = source.getmembers()
            if len(entries) > 10000:
                raise ValueError('Archive file-count limit exceeded')
            for entry in entries:
                total += entry.size
                if not safe_name(entry.name, stem) or not (entry.isdir() or entry.isfile()) or total > 536870912:
                    raise ValueError('Unsafe archive entry')
                path = directory.joinpath(*PurePosixPath(entry.name).parts)
                if entry.isdir():
                    path.mkdir(parents=True, exist_ok=True)
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    with source.extractfile(entry) as input_file, path.open('xb') as output:
                        shutil.copyfileobj(input_file, output)
                    path.chmod(0o755 if entry.mode & 0o111 else 0o644)
    else:
        with zipfile.ZipFile(archive) as source:
            entries = source.infolist()
            if len(entries) > 10000:
                raise ValueError('Archive file-count limit exceeded')
            for entry in entries:
                total += entry.file_size
                mode = stat.S_IFMT(entry.external_attr >> 16)
                if (not safe_name(entry.filename, stem) or total > 536870912
                        or mode not in (0, stat.S_IFREG, stat.S_IFDIR)):
                    raise ValueError('Unsafe archive entry')
            if source.testzip() is not None:
                raise ValueError('Archive CRC check failed')
            if system == 'windows':
                for entry in entries:
                    path = directory.joinpath(*PurePosixPath(entry.filename).parts)
                    if entry.is_dir():
                        path.mkdir(parents=True, exist_ok=True)
                    else:
                        path.parent.mkdir(parents=True, exist_ok=True)
                        with source.open(entry) as input_file, path.open('xb') as output:
                            shutil.copyfileobj(input_file, output)
        if system == 'macos':
            subprocess.run(['/usr/bin/ditto', '-x', '-k', str(archive), str(directory)], check=True)


def main() -> None:
    system, arch = host_target()
    source_hash = source_digest()
    suffix = '.tar.gz' if system == 'linux' else '.zip'
    archives = list((ROOT / 'dist/packages').glob(f'Maris-*-{system}-{arch}-local-{source_hash[:12]}{suffix}'))
    if len(archives) != 1:
        raise ValueError('Expected exactly one current-source native archive')
    archive = archives[0]
    stem = archive.name[:-len(suffix)]
    checksum = archive.parent / (stem + '.sha256')
    if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum.read_text(encoding='utf-8').split()[0]:
        raise ValueError('Archive checksum mismatch')
    review = ROOT / '.maris-review'
    review.mkdir(exist_ok=True)
    # Windows enforces a user-profile prefix, not the potentially machine-wide Actions checkout.
    temporary_root = None if system == 'windows' else review
    with tempfile.TemporaryDirectory(prefix='install-smoke-', dir=temporary_root) as temporary:
        directory = Path(temporary).resolve()
        extract(archive, directory, system, stem)
        kit = directory / stem
        metadata = json.loads((kit / 'BUILD.json').read_text(encoding='utf-8'))
        if (metadata['source_sha256'] != source_hash or metadata['platform'] != system
                or metadata['architecture'] != arch or metadata['commercial_release_approved'] is not False):
            raise ValueError('Artifact provenance mismatch')
        prefix = directory / 'Install with spaces'
        if system == 'windows':
            command = ['powershell.exe', '-NoProfile', '-NonInteractive', '-File', str(kit / 'install.ps1'),
                       '-From', str(kit / 'Maris'), '-Prefix', str(prefix), '-AllowUnsigned']
            dry, yes = '-DryRun', '-Yes'
            installed = prefix / 'Maris/bin/maris.exe'
        else:
            payload = kit / ('Maris.app' if system == 'macos' else 'Maris')
            command = ['/bin/bash', str(kit / 'install.sh'), '--from', str(payload), '--prefix', str(prefix), '--allow-unsigned']
            dry, yes = '--dry-run', '--yes'
            installed = prefix / ('Maris.app/Contents/MacOS/maris' if system == 'macos' else 'lib/maris/bin/maris')
        subprocess.run(command + [dry], check=True, capture_output=True, timeout=120)
        if prefix.exists():
            raise ValueError('Dry run created the installation prefix')
        subprocess.run(command + [yes], check=True, capture_output=True, timeout=120)
        if hashlib.sha256(installed.read_bytes()).hexdigest() != metadata['executable_sha256']:
            raise ValueError('Installed executable differs from the built artifact')
        result = subprocess.run([str(installed), '--version'], check=True, capture_output=True, text=True, timeout=30,
                                env=dict(os.environ, MARIS_STATE_DIR=str(directory / 'isolated-state')))
        if result.stdout.strip() != 'maris ' + metadata['version']:
            raise ValueError('Installed executable version mismatch')
    if source_digest() != source_hash:
        raise ValueError('Source changed during artifact validation')
    report = {'source_sha256': source_hash, 'platform': system, 'architecture': arch,
              'archive': archive.name, 'checksum_verified': True, 'dry_run_no_writes': True,
              'isolated_install_passed': True, 'installed_version_passed': True,
              'user_installation_changed': False, 'audio_started': False, 'hardware_validation': False}
    (review / 'package-smoke.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, tarfile.TarError, zipfile.BadZipFile) as error:
        raise SystemExit(str(error)) from error
