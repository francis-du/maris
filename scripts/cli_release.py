"""Prepare the CLI/TUI release contract from actual current-source native CI outputs.

GUI publisher verification and future product/hardware acceptance remain separate.
This module never signs, compiles, publishes, or labels an unsigned file as signed.
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
from dependency_notices import target, validate as validate_notices
from package_smoke import extract
from portable_package import host_target, validate_binary
from release_gate import check_report
from release_bundle import commit_release_inputs, digest, inspect_kit, pack
from source_audit import ROOT, source_digest


def software_gate(payload: Path, source: str, system: str, arch: str, version: str, report: Path) -> dict:
    marker = (payload / '.maris-package').read_text(encoding='utf-8').splitlines()
    if marker != ['maris-package-v2', system, arch, version, 'cli']:
        raise ValueError('A GUI or mismatched payload cannot use CLI release acceptance')
    binary = payload / 'bin' / ('maris.exe' if system == 'windows' else 'maris')
    validate_binary(binary, system, arch)
    updater = payload / 'resources' / ('install.ps1' if system == 'windows' else 'install.sh')
    if not updater.is_file() or updater.is_symlink() or updater.stat().st_size == 0:
        raise ValueError('CLI release is missing its local trusted updater')
    data = json.loads(report.read_text(encoding='utf-8'))
    check_report(data, source, report.parent, sys.platform)
    smoke_path = ROOT / '.maris-review/package-smoke.json'
    smoke = json.loads(smoke_path.read_text(encoding='utf-8'))
    if (smoke.get('source_sha256') != source or smoke.get('platform') != system or smoke.get('architecture') != arch
            or smoke.get('interface') != 'cli' or smoke.get('binary_sha256') != digest(binary)
            or not all(smoke.get(key) is True for key in ('checksum_verified', 'dry_run_no_writes',
                'isolated_install_passed', 'installed_version_passed', 'upgrade_passed', 'failed_upgrade_preserved_previous'))
            or smoke.get('audio_started') is not False or smoke.get('user_installation_changed') is not False):
        raise ValueError('Missing matching native CLI installation, upgrade and failure-recovery evidence')
    notice_hash = validate_notices(payload / 'resources/notices', target(system, arch))
    licensed = (ROOT / 'LICENSE').is_file()
    owner_license = payload / ('resources/MARIS_LICENSE.txt' if licensed else 'resources/MARIS_LICENSE_STATUS.txt')
    if not owner_license.is_file() or owner_license.is_symlink() or not 0 < owner_license.stat().st_size <= 1_048_576:
        raise ValueError('Project license status is missing from the package')
    if licensed and digest(owner_license) != digest(ROOT / 'LICENSE'):
        raise ValueError('Packaged project terms differ from the source license')
    if not licensed and owner_license.read_text(encoding='utf-8') != (
            'This source revision declares no project license grant. This package does not add an open-source license grant.\n'
            'Third-party components retain the license terms reproduced in resources/notices.\n'):
        raise ValueError('Package invents a project license grant absent from this source')
    return {'source_sha256': source, 'platform': system, 'architecture': arch, 'interface': 'cli',
            'release_ready': True, 'published': False, 'publisher_signed': False,
            'completed_rounds': data['completed_rounds'], 'round_report_sha256': digest(report),
            'package_smoke_passed': True, 'package_smoke_sha256': digest(smoke_path),
            'notice_index_sha256': notice_hash, 'project_license_sha256': digest(owner_license),
            'project_license_status': 'declared' if licensed else 'unspecified',
            'hardware_validation': False, 'subjective_listening_validation': False,
            'scope': 'Native CLI/TUI archives, checksums and isolated install/upgrade/recovery; no GUI signing or physical-device certification.'}


def prepare(report: Path | None) -> dict:
    system, arch = host_target()
    source = source_digest()
    version = re.search(r'^version = "([0-9.]+)"$', (ROOT / 'Cargo.toml').read_text(encoding='utf-8'), re.M).group(1)
    reports = [report] if report else list((ROOT / '.maris-review/verification').glob('*/report.json'))
    if len(reports) != 1:
        raise ValueError('CLI preparation requires exactly one actual current-source 200-round report')
    report = reports[0].resolve()
    if not report.is_relative_to(ROOT.resolve()) or report.is_symlink():
        raise ValueError('Round evidence must belong to this source checkout')
    suffix = '.tar.gz' if system == 'linux' else '.zip'
    locals = list((ROOT / 'dist/packages').glob(f'Maris-{version}-{system}-{arch}-local-{source[:12]}{suffix}'))
    if len(locals) != 1:
        raise ValueError('Build and smoke-test the current native CLI package first')
    stem = f'Maris-{version}-{system}-{arch}'
    destination = ROOT / 'dist/online-approved'
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / (stem + ('.zip' if system == 'windows' else '.tar.gz'))
    record_path = destination / (stem + '.release.json')
    if archive.is_symlink() or record_path.is_symlink() or record_path.exists():
        raise ValueError('Refusing to overwrite release inputs')
    # A crash may have committed the exact archive hard-link before the matching record.
    # Rebuild and byte-verify that orphan below instead of making the release lane unrecoverable.
    with tempfile.TemporaryDirectory(prefix='cli-release-', dir=ROOT / '.maris-review') as temporary:
        work = Path(temporary)
        local_stem = locals[0].name[:-len(suffix)]
        extract(locals[0], work / 'local', system, local_stem)
        existing = work / 'local' / local_stem
        metadata = json.loads((existing / 'BUILD.json').read_text(encoding='utf-8'))
        if (metadata.get('source_sha256') != source or metadata.get('platform') != system
                or metadata.get('architecture') != arch or metadata.get('interface') != 'cli'):
            raise ValueError('The local archive does not implement this exact native CLI contract')
        payload = existing / 'Maris'
        proof = software_gate(payload, source, system, arch, version, report)
        kit = work / stem
        kit.mkdir()
        shutil.copytree(payload, kit / 'Maris')
        binary_hash = digest(kit / 'Maris/bin' / ('maris.exe' if system == 'windows' else 'maris'))
        (kit / '.maris-release').write_text('\n'.join(['maris-install-kit-v2', version, system, arch,
            source, 'stable', binary_hash, 'cli']) + '\n', encoding='ascii', newline='\n')
        (kit / 'CLI_VERIFICATION.json').write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8', newline='\n')
        if system == 'windows':
            shutil.copyfile(ROOT / 'install.ps1', kit / 'install.ps1')
        else:
            shutil.copyfile(ROOT / 'install.sh', kit / 'install.sh')
            (kit / 'scripts').mkdir()
            shutil.copyfile(ROOT / 'scripts/install_cli.sh', kit / 'scripts/install_cli.sh')
        shutil.copyfile(ROOT / 'docs/en/install.md', kit / 'INSTALL.md')
        packed = work / archive.name
        # CLI kits contain ordinary portable files. GUI metadata/signature preservation
        # remains in the legacy macOS application-kit path.
        pack(kit, packed, 'windows' if system == 'windows' else 'linux')
        restored = work / 'restored'
        extract(packed, restored, 'windows' if system == 'windows' else 'linux', stem)
        software_gate(restored / stem / 'Maris', source, system, arch, version, report)
        if source_digest() != source:
            raise ValueError('Source changed while preparing the CLI release')
        record = {'schema_version': 2, 'interface': 'cli', 'version': version, 'platform': system, 'architecture': arch,
                  'source_sha256': source, 'channel': 'stable', 'archive': archive.name,
                  'sha256': digest(packed), 'bytes': packed.stat().st_size, 'binary_sha256': binary_hash,
                  'native_gate': proof, 'ci_run': os.environ.get('GITHUB_RUN_ID'),
                  'ci_commit': os.environ.get('GITHUB_SHA'), 'ci_repository': os.environ.get('GITHUB_REPOSITORY')}
        inspect_kit(packed, record)
        record_ready = work / record_path.name
        with record_ready.open('x', encoding='utf-8', newline='\n') as handle:
            handle.write(json.dumps(record, indent=2) + '\n')
            handle.flush()
            os.fsync(handle.fileno())
        commit_release_inputs(packed, record_ready, archive, record_path, record['sha256'])
    return {'archive': str(archive.relative_to(ROOT)), 'interface': 'cli', 'channel': 'stable', 'compiled_here': False, 'published': False}
