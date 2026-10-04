#!/usr/bin/env python3
"""Prepare CI-built online-install kits or combine their manifests. Never compile or publish.

Candidate kits are deliberately unusable by the default stable installer. Stable
kits require the native release preflight on the final signed/reviewed payload.
"""
from __future__ import annotations
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from portable_package import host_target
from package_smoke import extract
from release_requirements import blockers
from source_audit import ROOT, source_digest

TARGETS = {(system, arch) for system in ('macos', 'linux', 'windows') for arch in ('x86_64', 'arm64')}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def commit_release_inputs(
    packed: Path,
    record_ready: Path,
    archive: Path,
    record_path: Path,
    expected_sha256: str,
) -> None:
    """Atomically converge an exact archive/record pair after races or process crashes."""
    if archive.exists():
        if archive.is_symlink() or archive.stat().st_size != packed.stat().st_size or digest(archive) != expected_sha256:
            raise ValueError('Existing release archive differs from the exact rebuilt input')
    else:
        try:
            os.link(packed, archive)
        except FileExistsError:
            if archive.is_symlink() or archive.stat().st_size != packed.stat().st_size or digest(archive) != expected_sha256:
                raise ValueError('Existing release archive differs from the exact rebuilt input')
    try:
        os.link(record_ready, record_path)
    except FileExistsError:
        if record_path.is_symlink() or record_path.read_bytes() != record_ready.read_bytes():
            raise ValueError('Existing release record differs from the exact rebuilt input')


def inspect_kit(archive: Path, item: dict) -> None:
    """Bind catalog metadata to the actual package before any publication manifest exists."""
    system, arch, version = item['platform'], item['architecture'], item['version']
    cli = item.get('interface') == 'cli'
    if item.get('native_gate') is not None and not isinstance(item['native_gate'], dict):
        raise ValueError('Invalid native acceptance record')
    stem = f'Maris-{version}-{system}-{arch}'
    payload = 'Maris.app' if system == 'macos' and not cli else 'Maris'
    executable = 'Contents/MacOS/maris' if system == 'macos' and not cli else ('bin/maris.exe' if system == 'windows' else 'bin/maris')
    binary_name = f'{stem}/{payload}/{executable}'
    marker_name = f'{stem}/.maris-release'
    identity_name = f'{stem}/{payload}/' + ('Contents/Info.plist' if system == 'macos' and not cli else '.maris-package')
    helper_name = f'{stem}/install.ps1' if system == 'windows' else f'{stem}/scripts/install_{"cli" if cli else system}.sh'
    proof_name = f'{stem}/CLI_VERIFICATION.json'
    notice_name = f'{stem}/{payload}/resources/notices/index.json'
    license_name = f'{stem}/{payload}/resources/' + ('MARIS_LICENSE.txt' if item.get('native_gate', {}).get('project_license_status') == 'declared' else 'MARIS_LICENSE_STATUS.txt') if cli and item.get('native_gate') else ''
    acceptance = cli and bool(item.get('native_gate'))
    seen, spelling, kinds = set(), {}, {}
    captured = {}
    file_hashes = {}
    total = count = 0

    def inspect(name, size, mode, directory, open_entry):
        nonlocal count, total
        count += 1; total += size
        if count > 10000 or size < 0 or total > 536870912:
            raise ValueError('Release kit exceeds the installer entry/expanded-size bounds')
        path = name[:-1] if name.endswith('/') else name
        if (not re.fullmatch(r'[A-Za-z0-9_./-]+', path)
                or not (path == stem or path.startswith(stem + '/'))):
            raise ValueError('Unsafe release kit path: ' + repr(path[:160]))
        parts = path.split('/')
        for index, part in enumerate(parts):
            if (part in ('', '.', '..') or part.endswith('.')
                    or (system == 'windows' and re.fullmatch(r'(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?', part, re.I))):
                raise ValueError('Ambiguous or reserved release kit path')
            prefix = '/'.join(parts[:index + 1]); key = prefix.lower()
            if key in spelling and spelling[key] != prefix:
                raise ValueError('Case-colliding release kit paths')
            spelling[key] = prefix
            is_directory = directory or index < len(parts) - 1
            if key in kinds and kinds[key] != is_directory:
                raise ValueError('A release kit file collides with a directory')
            kinds[key] = is_directory
        if path.lower() in seen:
            raise ValueError('Duplicate release kit entry')
        seen.add(path.lower())
        if mode & 0o7000:
            raise ValueError('Privileged archive permissions rejected')
        if directory:
            if size:
                raise ValueError('Directory payload rejected')
            return
        relevant = path in (binary_name, marker_name, identity_name, helper_name) or acceptance and path in (proof_name, notice_name, license_name)
        if not relevant and not acceptance:
            return
        bound = 134217728 if path == binary_name else (1048576 if path in (helper_name, notice_name, license_name) else (65536 if relevant else 536870912))
        if not 0 < size <= bound:
            raise ValueError('Missing or oversized executable/installer/identity')
        hasher = hashlib.sha256(); chunks = []; read = 0
        with open_entry() as source:
            while True:
                chunk = source.read(min(65536, bound + 1 - read))
                if not chunk:
                    break
                read += len(chunk)
                if read > bound or read > size:
                    raise ValueError('Release entry exceeds its declared size')
                hasher.update(chunk)
                if relevant and path != binary_name:
                    chunks.append(chunk)
        if read != size:
            raise ValueError('Truncated release kit entry')
        file_hashes[path] = hasher.hexdigest()
        if relevant:
            captured[path] = hasher.hexdigest() if path == binary_name else b''.join(chunks)

    try:
        if system == 'windows':
            with zipfile.ZipFile(archive) as source:
                for entry in source.infolist():
                    mode = entry.external_attr >> 16
                    if stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR) or entry.external_attr & 0x400:
                        raise ValueError('Linked or special release kit entry')
                    inspect(entry.filename, entry.file_size, mode, entry.is_dir(), lambda e=entry: source.open(e))
        else:
            with tarfile.open(archive, 'r:gz') as source:
                for entry in source:
                    if not (entry.isdir() or entry.isfile()):
                        raise ValueError('Linked or special release kit entry')
                    inspect(entry.name, entry.size, entry.mode, entry.isdir(), lambda e=entry: source.extractfile(e))
        if not all(name in captured for name in (binary_name, marker_name, identity_name, helper_name)):
            raise ValueError('Release kit is missing its executable, installer or identity')
        expected = '\n'.join(['maris-install-kit-v2' if cli else 'maris-install-kit-v1', version, system, arch,
                              item['source_sha256'], item['channel'], item['binary_sha256']] + (['cli'] if cli else [])) + '\n'
        if captured[marker_name] != expected.encode('ascii') or captured[binary_name] != item['binary_sha256']:
            raise ValueError('Release record differs from embedded identity or executable')
        if system == 'macos' and not cli:
            metadata = plistlib.loads(captured[identity_name])
            if metadata.get('CFBundleShortVersionString') != version:
                raise ValueError('Bundle and catalog versions differ')
        elif captured[identity_name] != (f'maris-package-v2\n{system}\n{arch}\n{version}\ncli\n' if cli else f'maris-package-v1\n{system}\n{arch}\n{version}\n').encode('ascii'):
            raise ValueError('Native payload identity differs from catalog')
        if acceptance:
            proof = item['native_gate']
            if (json.loads(captured.get(proof_name, b'null')) != proof
                    or file_hashes.get(notice_name) != proof.get('notice_index_sha256')
                    or file_hashes.get(license_name) != proof.get('project_license_sha256')):
                raise ValueError('CLI acceptance differs from the actual archived proof, notices or license status')
            notices = json.loads(captured[notice_name])
            from dependency_notices import notice_entries, target
            for entry in notice_entries(notices, target(system, arch)):
                name = f'{stem}/{payload}/resources/notices/' + entry['path']
                if file_hashes.get(name) != entry['sha256']:
                    raise ValueError('Actual archived dependency notice is missing or modified')
    except (tarfile.TarError, zipfile.BadZipFile, EOFError, plistlib.InvalidFileException) as error:
        raise ValueError('Unreadable release kit') from error


def combine(directory: Path, output: Path, root: Path = ROOT, *, expected_ci: dict | None = None) -> str:
    records = []
    for file in sorted(directory.rglob('*.release.json')):
        if file.is_symlink():
            raise ValueError('Linked release record rejected')
        if file.stat().st_size > 32768:
            raise ValueError('Oversized release record')
        item = json.loads(file.read_text(encoding='utf-8'))
        if (item.get('schema_version') not in (1, 2) or item.get('channel') not in ('candidate', 'stable')
                or (item.get('schema_version') == 2 and item.get('interface') != 'cli')
                or (item.get('schema_version') == 1 and item.get('interface') not in (None, 'gui'))):
            raise ValueError('Unsupported release record schema or channel')
        version, system, arch = item['version'], item['platform'], item['architecture']
        suffix = '.zip' if system == 'windows' else '.tar.gz'
        name = f'Maris-{version}-{system}-{arch}{suffix}'
        if not re.fullmatch(r'\d+\.\d+\.\d+', version) or (system, arch) not in TARGETS or item['archive'] != name:
            raise ValueError('Invalid native release identity')
        archive = file.parent / name
        if archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size <= 134217728:
            raise ValueError('Missing, linked or oversized release archive')
        if archive.stat().st_size != item['bytes'] or digest(archive) != item['sha256']:
            raise ValueError('Release archive changed after preparation')
        if not re.fullmatch('[a-f0-9]{64}', item['binary_sha256']) or not re.fullmatch('[a-f0-9]{64}', item['source_sha256']):
            raise ValueError('Invalid release digests')
        inspect_kit(archive, item)
        records.append(item)
    if len(records) != 6 or {(r['platform'], r['architecture']) for r in records} != TARGETS:
        raise ValueError('All six distinct native CI targets are required')
    versions = {r['version'] for r in records}; sources = {r['source_sha256'] for r in records}; channels = {r['channel'] for r in records}
    if len(versions) != 1 or len(sources) != 1 or len(channels) != 1 or not channels <= {'candidate', 'stable'}:
        raise ValueError('Mixed source revisions, versions or channels rejected')
    interfaces = {r.get('interface', 'gui') for r in records}
    if len(interfaces) != 1:
        raise ValueError('Mixed CLI and GUI distribution contracts rejected')
    cli = interfaces == {'cli'}
    identities = {(r.get('ci_run'), r.get('ci_commit'), r.get('ci_repository')) for r in records}
    if len(identities) != 1:
        raise ValueError('Mixed CI runs, commits or repositories rejected')
    run, commit, repository = next(iter(identities))
    if any(value is not None for value in (run, commit, repository)):
        if (not isinstance(run, str) or not re.fullmatch(r'[1-9][0-9]*', run)
                or not isinstance(commit, str) or not re.fullmatch(r'[a-f0-9]{40}', commit)
                or not isinstance(repository, str) or not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository)):
            raise ValueError('Incomplete CI provenance')
    if expected_ci is not None:
        if (run, commit, repository) != (expected_ci.get('run'), expected_ci.get('commit'), expected_ci.get('repository')) or run is None:
            raise ValueError('Kits were not produced by this exact CI run and commit')
        if next(iter(sources)) != source_digest():
            raise ValueError('CI kits differ from the checked-out source')
    channel = records[0]['channel']
    if channel == 'stable':
        if cli and expected_ci is None:
            raise ValueError('CLI publication requires the exact six-target CI run identity')
        if not cli and blockers(root):
            raise ValueError('Unfinished product requirements prohibit a stable manifest')
        if next(iter(sources)) != source_digest():
            raise ValueError('Approved kits do not match the current source')
        for record in records:
            proof = record.get('native_gate', {})
            if (proof.get('release_ready') is not True or proof.get('published') is not False
                    or proof.get('source_sha256') != record['source_sha256']
                    or proof.get('platform') != record['platform']
                    or proof.get('architecture') != record['architecture']):
                raise ValueError('Missing matching native signature and acceptance preflight')
            if cli and (proof.get('interface') != 'cli' or proof.get('publisher_signed') is not False
                        or proof.get('completed_rounds', 0) < 200 or proof.get('package_smoke_passed') is not True
                        or not re.fullmatch('[a-f0-9]{64}', str(proof.get('notice_index_sha256', '')))):
                raise ValueError('Missing native CLI software, notice and installation evidence')
    lines = ['maris-release-v2' if cli else 'maris-release-v1', 'version\t' + records[0]['version'],
             'source_sha256\t' + records[0]['source_sha256'], 'channel\t' + channel]
    if cli:
        lines.append('interface\tcli')
    for item in sorted(records, key=lambda r: (r['platform'], r['architecture'])):
        lines.append('\t'.join(['asset', item['platform'], item['architecture'], item['archive'],
                                item['sha256'], str(item['bytes']), item['binary_sha256']]))
    if output.exists() or output.is_symlink():
        raise ValueError('Manifest destination already exists')
    output.parent.mkdir(parents=True, exist_ok=True)
    payload = ('\n'.join(lines) + '\n').encode('ascii')
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix='.maris-release.', dir=output.parent, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        # Hard-link publication is an atomic no-overwrite commit on the destination
        # filesystem: concurrent collectors cannot both win the earlier existence check.
        os.link(temporary, output)
    except FileExistsError as error:
        raise ValueError('Manifest destination already exists') from error
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return channel


def pack(kit: Path, archive: Path, system: str) -> None:
    if system == 'windows':
        with zipfile.ZipFile(archive, 'x', zipfile.ZIP_DEFLATED) as output:
            for path in sorted(kit.rglob('*')):
                if path.is_symlink():
                    raise ValueError('Linked kit entry rejected')
                if path.is_file():
                    info = zipfile.ZipInfo(f'{kit.name}/{path.relative_to(kit).as_posix()}')
                    info.date_time = (1980, 1, 1, 0, 0, 0)
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.create_system = 3
                    info.external_attr = 0o100644 << 16
                    output.writestr(info, path.read_bytes())
    elif system == 'macos':
        # Native bsdtar preserves application metadata. The same native gate is rerun
        # after extraction below; lost signing/stapling data cannot produce a stable kit.
        env = {k: v for k, v in os.environ.items()
               if k not in ('TAR_OPTIONS', 'GZIP', 'GZIP_OPT', 'COPYFILE_DISABLE')}
        # Archive the contents with their root prefix, not the newly created wrapper
        # directory itself. Apple's ._wrapper metadata otherwise lands beside the
        # permitted package root. Payload metadata remains inside that root and is
        # deliberately preserved; no signing/quarantine attribute is stripped.
        members = [f'{kit.name}/{path.name}' for path in sorted(kit.iterdir())]
        if not members:
            raise ValueError('Empty macOS kit')
        subprocess.run(['/usr/bin/tar', '--mac-metadata', '--format', 'pax', '-czf', str(archive),
                        '-C', str(kit.parent), *members], check=True, env=env, timeout=120)
    else:
        def clean(info):
            if not (info.isfile() or info.isdir()):
                raise ValueError('Linked or special kit entry rejected')
            info.uid = info.gid = 0; info.uname = info.gname = ''; info.mtime = 0
            return info
        with archive.open('xb') as raw:
            with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as output:
                    output.add(kit, arcname=kit.name, filter=clean)


def gate(args, bundle: Path) -> dict:
    command = [sys.executable, str(ROOT / 'scripts/release_gate.py'), '--report', str(args.report),
               '--approval', str(args.approval), '--bundle', str(bundle)]
    for field, flag in [('signature', '--signature'), ('digest_file', '--digest-file'), ('keyring', '--keyring')]:
        value = getattr(args, field)
        if value is not None:
            command += [flag, str(value)]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise ValueError('Native release preflight blocked the kit: ' + result.stdout.strip() + result.stderr.strip())
    data = json.loads(result.stdout)
    if data.get('release_ready') is not True:
        raise ValueError('Native release preflight did not approve the final payload')
    return data


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', action='store_true')
    parser.add_argument('--cli', action='store_true', help='Prepare a checksum-verified CLI/TUI stable kit; GUI gates are separate')
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--approval', type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--signature', type=Path)
    parser.add_argument('--digest-file', type=Path)
    parser.add_argument('--keyring', type=Path)
    parser.add_argument('--collect', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.cli:
        if args.candidate or args.collect or args.output or args.bundle or args.approval or args.signature or args.digest_file or args.keyring:
            parser.error('--cli uses only the current native CI package and its actual software reports')
        from cli_release import prepare
        print(json.dumps(prepare(args.report)))
        return
    if args.collect is not None:
        if args.output is None or args.candidate or any([args.bundle, args.approval, args.report]):
            parser.error('--collect requires only --output')
        expected_ci = None
        if os.environ.get('GITHUB_ACTIONS') == 'true':
            expected_ci = {'run': os.environ.get('GITHUB_RUN_ID'), 'commit': os.environ.get('GITHUB_SHA'),
                           'repository': os.environ.get('GITHUB_REPOSITORY')}
        channel = combine(args.collect, args.output, expected_ci=expected_ci)
        print(json.dumps({'manifest': str(args.output), 'channel': channel, 'published': False}))
        return
    if args.output is not None or (args.candidate and any([args.bundle, args.approval, args.report])):
        parser.error('Candidate creation cannot carry approval or output overrides')
    if not args.candidate and not all([args.bundle, args.approval, args.report]):
        parser.error('Stable kit preparation requires --bundle, --approval and --report')
    system, arch = host_target(); source = source_digest()
    version = re.search(r'^version = "([0-9.]+)"$', (ROOT / 'Cargo.toml').read_text(), re.M).group(1)
    stem = f'Maris-{version}-{system}-{arch}'
    channel = 'candidate' if args.candidate else 'stable'
    destination = ROOT / 'dist' / ('online-candidate' if args.candidate else 'online-approved')
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / (stem + ('.zip' if system == 'windows' else '.tar.gz'))
    record_path = destination / (stem + '.release.json')
    if archive.is_symlink() or record_path.is_symlink() or record_path.exists():
        raise ValueError('Online kit already exists; never overwrite release inputs')
    # An archive without its matching record can be left by process death between
    # the two final hard links. Rebuild first and recover it only if bytes are exact.
    review = ROOT / '.maris-review'; review.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='online-kit-', dir=review) as temporary:
        work = Path(temporary)
        if args.candidate:
            suffix = '.tar.gz' if system == 'linux' else '.zip'
            local = list((ROOT / 'dist/packages').glob(f'Maris-{version}-{system}-{arch}-local-{source[:12]}{suffix}'))
            if len(local) != 1:
                raise ValueError('Build the current-source native CI package first; this tool never compiles')
            local_stem = local[0].name[:-len(suffix)]
            extract(local[0], work / 'local', system, local_stem)
            existing = work / 'local' / local_stem
            metadata = json.loads((existing / 'BUILD.json').read_text())
            if metadata['source_sha256'] != source or metadata['platform'] != system or metadata['architecture'] != arch:
                raise ValueError('CI package provenance differs from the current source/host')
            payload = existing / ('Maris.app' if system == 'macos' else 'Maris')
            proof = None
        else:
            payload = args.bundle.resolve()
            payload.relative_to(ROOT.resolve())
            proof = gate(args, payload)
        kit = work / stem; kit.mkdir()
        payload_name = 'Maris.app' if system == 'macos' else 'Maris'
        shutil.copytree(payload, kit / payload_name, symlinks=False)
        binary = kit / payload_name / ('Contents/MacOS/maris' if system == 'macos' else ('bin/maris.exe' if system == 'windows' else 'bin/maris'))
        binary_hash = digest(binary)
        (kit / '.maris-release').write_text('\n'.join(['maris-install-kit-v1', version, system, arch, source, channel, binary_hash]) + '\n', encoding='ascii')
        if system == 'windows':
            shutil.copyfile(ROOT / 'install.ps1', kit / 'install.ps1')
        else:
            (kit / 'scripts').mkdir()
            shutil.copyfile(ROOT / 'install.sh', kit / 'install.sh')
            for helper in ('install_macos.sh', 'install_linux.sh'):
                shutil.copyfile(ROOT / 'scripts' / helper, kit / 'scripts' / helper)
        shutil.copyfile(ROOT / 'docs/en/install.md', kit / 'INSTALL.md')
        packed = work / archive.name
        pack(kit, packed, system)
        if not 0 < packed.stat().st_size <= 134217728:
            raise ValueError('Online kit exceeds the bootstrap transfer bound')
        if not args.candidate:
            # Verify the actual packaged payload, not only the pre-archive input.
            restored = work / 'restored'; restored.mkdir()
            if system == 'macos':
                subprocess.run(['/usr/bin/tar', '--mac-metadata', '-xpf', str(packed), '-C', str(restored)], check=True, timeout=120)
            else:
                extract(packed, restored, system, stem)
            proof = gate(args, restored / stem / payload_name)
        if source_digest() != source:
            raise ValueError('Source changed during kit preparation')
        record = {'schema_version': 1, 'version': version, 'source_sha256': source, 'platform': system,
                  'architecture': arch, 'channel': channel, 'archive': archive.name, 'sha256': digest(packed),
                  'bytes': packed.stat().st_size, 'binary_sha256': binary_hash, 'native_gate': proof,
                  'ci_run': os.environ.get('GITHUB_RUN_ID'), 'ci_commit': os.environ.get('GITHUB_SHA'),
                  'ci_repository': os.environ.get('GITHUB_REPOSITORY')}
        inspect_kit(packed, record)
        record_ready = work / record_path.name
        with record_ready.open('x', encoding='utf-8', newline='\n') as handle:
            handle.write(json.dumps(record, indent=2) + '\n')
            handle.flush()
            os.fsync(handle.fileno())
        # Final publication is exact-byte idempotent. This makes a rerun recover
        # an orphan archive left by process death and lets identical concurrent
        # preparers converge without replacing either final path.
        commit_release_inputs(packed, record_ready, archive, record_path, record['sha256'])
    print(json.dumps({'archive': str(archive.relative_to(ROOT)), 'channel': channel, 'compiled_here': False, 'published': False}))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.SubprocessError, tarfile.TarError, zipfile.BadZipFile) as error:
        raise SystemExit(str(error)) from error
