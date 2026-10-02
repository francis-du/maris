#!/usr/bin/env python3
"""Collect the locked native runtime's actual license texts without inventing clearance."""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def target(system: str, arch: str) -> str:
    machine = 'aarch64' if arch == 'arm64' else 'x86_64'
    return machine + {'macos': '-apple-darwin', 'linux': '-unknown-linux-gnu', 'windows': '-pc-windows-msvc'}[system]


def collect(root: Path, output: Path, system: str, arch: str) -> dict:
    native = target(system, arch)
    tree = subprocess.run(['cargo', 'tree', '--locked', '--target', native, '--edges', 'normal',
                           '--prefix', 'none', '--format', '{p}'], cwd=root, check=True,
                          capture_output=True, text=True, encoding='utf-8', timeout=180).stdout
    identities = {tuple(match.groups()) for line in tree.splitlines()
                  if (match := re.match(r'^([A-Za-z0-9_-]+) v([^ ]+)', line))}
    metadata = json.loads(subprocess.run(['cargo', 'metadata', '--locked', '--format-version', '1',
                                        '--filter-platform', native], cwd=root, check=True,
                                        capture_output=True, text=True, encoding='utf-8', timeout=180).stdout)
    fallback_path = root / 'third_party/rust-notices/index.json'
    fallbacks = json.loads(fallback_path.read_text(encoding='utf-8')) if fallback_path.exists() else {}
    if output.exists() or output.is_symlink():
        raise ValueError('Notice output already exists')
    output.mkdir(parents=True)
    revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=root, check=True,
                              capture_output=True, text=True, encoding='utf-8').stdout.strip()
    records = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['name'] == 'maris' or (package['name'], package['version']) not in identities:
            continue
        expression = package.get('license')
        if not expression:
            raise ValueError('Missing dependency license declaration: ' + package['name'])
        directory = Path(package['manifest_path']).parent
        files = [path for path in directory.iterdir() if path.is_file()
                 and path.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright', 'unlicense', 'authors'))]
        if package.get('license_file'):
            files.append(directory / package['license_file'])
        key = package['name'] + '@' + package['version']
        provenance = 'published locked Cargo package'
        if not files:
            fallback = fallbacks.get(key, {})
            if fallback.get('license') != expression or not fallback.get('files'):
                raise ValueError('Missing actual license text: ' + key)
            vcs = json.loads((directory / '.cargo_vcs_info.json').read_text(encoding='utf-8'))
            if fallback.get('revision') != vcs['git']['sha1']:
                raise ValueError('License fallback is not pinned to the published crate: ' + key)
            files = []
            for entry in fallback['files']:
                path = root / 'third_party/rust-notices' / entry['path']
                if not path.resolve().is_relative_to((root / 'third_party/rust-notices').resolve()) or sha(path) != entry['sha256']:
                    raise ValueError('Modified license fallback: ' + key)
                files.append(path)
            provenance = fallback['repository'] + '@' + fallback['revision']
        notices = []
        for index, path in enumerate(sorted(set(files))):
            if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 1_048_576:
                raise ValueError('Missing, linked or oversized license text: ' + key)
            path.read_text(encoding='utf-8')
            name = f'rust/{package["name"]}-{package["version"]}/{index:02d}-{path.name}'
            destination = output / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            notices.append({'path': name, 'sha256': sha(destination)})
        record = {'name': package['name'], 'version': package['version'], 'license': expression,
                  'repository': package.get('repository'), 'provenance': provenance, 'files': notices,
                  'published_source_archive': f'https://crates.io/api/v1/crates/{package["name"]}/{package["version"]}/download'}
        if directory.resolve().is_relative_to(root.resolve()):
            record['modified_source'] = f'https://github.com/francis-du/maris/tree/{revision}/{directory.resolve().relative_to(root.resolve()).as_posix()}'
        records.append(record)
    if {p['name'] for p in records} != {name for name, _ in identities if name != 'maris'}:
        raise ValueError('Runtime dependency inventory is incomplete')
    assets = []
    for family in ('eqmac', 'autoeq', 'musicnn', 'flexaudio-core'):
        directory = root / 'third_party' / family
        if not directory.is_dir():
            raise ValueError('Missing bundled third-party provenance: ' + family)
        for path in sorted(directory.iterdir()):
            if path.is_file() and path.name.lower().startswith(('license', 'copying', 'notice', 'source', 'readme')):
                name = f'assets/{family}/{path.name}'
                destination = output / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, destination)
                assets.append({'path': name, 'sha256': sha(destination)})
    result = {'schema_version': 1, 'target': native, 'runtime_dependencies': records, 'bundled_assets': assets,
              'scope': 'Locked native runtime dependency and bundled asset notices; no publisher signature or hardware claim.'}
    (output / 'index.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8', newline='\n')
    validate(output, native)
    return result


def validate(directory: Path, native: str) -> str:
    index = directory / 'index.json'
    record = json.loads(index.read_text(encoding='utf-8'))
    if record.get('schema_version') != 1 or record.get('target') != native or not record.get('runtime_dependencies'):
        raise ValueError('Incomplete native dependency notice index')
    entries = [entry for package in record['runtime_dependencies'] for entry in package['files']] + record['bundled_assets']
    if any(not package.get('license') or not package.get('files') for package in record['runtime_dependencies']):
        raise ValueError('Dependency has no license expression or actual notice text')
    for entry in entries:
        path = directory / entry['path']
        if (not path.resolve().is_relative_to(directory.resolve()) or path.is_symlink() or not path.is_file()
                or sha(path) != entry['sha256']):
            raise ValueError('Missing, linked or modified dependency notice text')
    return sha(index)
