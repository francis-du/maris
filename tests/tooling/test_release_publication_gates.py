"""Execute release workflow gates with isolated files and a fake GitHub client.

No network, release, tag, upload, installer or audio action is performed.
"""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from test_ci import BASH

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / '.github/workflows/release-assets.yml'
PUBLISH_WORKFLOW = ROOT / '.github/workflows/publish-release.yml'
COMMIT = 'a' * 40
SOURCE = hashlib.sha256(b'audited release fixture').hexdigest()
TARGETS = [('macos', 'x86_64'), ('macos', 'arm64'), ('linux', 'x86_64'),
           ('linux', 'arm64'), ('windows', 'x86_64'), ('windows', 'arm64')]


def block(name, workflow=WORKFLOW):
    lines = workflow.read_text(encoding='utf-8').splitlines()
    start = lines.index('      - name: ' + name)
    run = next(i for i in range(start + 1, len(lines)) if lines[i] == '        run: |')
    result = []
    for line in lines[run + 1:]:
        if line.strip() and not line.startswith('          '):
            break
        result.append(line[10:] if line.strip() else '')
    return '\n'.join(result) + '\n'


@unittest.skipUnless(BASH and shutil.which('git'), 'Bash and Git fixture execution required')
class ReleasePublicationGates(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'checkout'
        self.runner = self.base / 'runner'
        self.root.mkdir()
        self.runner.mkdir()
        (self.root / 'scripts').mkdir()
        shutil.copyfile(ROOT / 'scripts/source_audit.py', self.root / 'scripts/source_audit.py')
        (self.root / 'Cargo.toml').write_text('[package]\nversion = "1.2.3"\n', encoding='utf-8')
        (self.root / '.gitignore').write_text(
            '.release-*/\n.maris-review/\n__pycache__/\n*.py[cod]\n', encoding='utf-8')
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True, capture_output=True)
        subprocess.run(['git', '-C', str(self.root), 'add', '.'], check=True, capture_output=True)
        self.run = {'id': 17, 'name': 'Build application artifacts',
                    'path': '.github/workflows/build.yml', 'head_sha': COMMIT,
                    'status': 'completed', 'conclusion': 'success', 'event': 'push',
                    'head_branch': 'release-validation/v1.2.3',
                    'repository': {'full_name': 'fixture/maris'},
                    'head_repository': {'full_name': 'fixture/maris'}}
        self.release = {'id': 29, 'tag_name': 'v1.2.3', 'draft': False,
                        'prerelease': False, 'published_at': '2026-01-01T00:00:00Z'}
        self.environment = dict(os.environ, GH_REPO='fixture/maris',
            GH_TOKEN='non-secret-offline-fixture', SOURCE_SHA=COMMIT, SOURCE_SHA256=SOURCE,
            SOURCE_RUN_ID='17', RELEASE_TAG='v1.2.3', RELEASE_TAG_INPUT='',
            RELEASE_ID_INPUT='', CALLED_FROM_PUBLISH='false', RELEASE_ID='29',
            GITHUB_SHA=COMMIT, MAIN_SHA=COMMIT, RELEASE_VERSION='1.2.3',
            GITHUB_OUTPUT=(self.runner / 'outputs').as_posix(), RUNNER_TEMP=self.runner.as_posix(),
            FIXTURE_RUNNER=self.runner.as_posix(), FIXTURE_PYTHON=Path(sys.executable).as_posix(),
            TAG_SHA=COMMIT, FIXTURE_VERSION_TAGS='v1.2.3')
        (self.runner / 'gh_fixture.py').write_text(self.gh_client(), encoding='utf-8', newline='\n')
        # Always use the invoking interpreter, including Git Bash on Windows.
        for name, command in (
                ('gh', 'exec "$FIXTURE_PYTHON" "$FIXTURE_RUNNER/gh_fixture.py" "$@"'),
                ('python3', 'exec "$FIXTURE_PYTHON" "$@"')):
            (self.runner / name).write_text('#!/bin/sh\n' + command + '\n',
                                             encoding='utf-8', newline='\n')
            (self.runner / name).chmod(0o755)
        self.environment['PATH'] = str(self.runner) + os.pathsep + os.environ.get('PATH', '')

    def gh_client(self):
        return """#!/usr/bin/env python3
import json, os
from pathlib import Path
import shutil, sys
root = Path(os.environ['FIXTURE_RUNNER'])
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as stream:
    stream.write(json.dumps(args) + '\\n')
if args[0] == 'api':
    route = next(value for value in args if value.startswith('repos/'))
    method = args[args.index('--method') + 1] if '--method' in args else 'GET'
    if method == 'DELETE':
        if os.environ.get('FAIL_DELETE_ROUTE') == route:
            sys.exit(23)
    elif method == 'PATCH':
        pass
    elif method == 'POST' and route.endswith('/git/refs'):
        if os.environ.get('FAIL_TAG_CREATE'):
            sys.exit(24)
        print('{}')
    elif method == 'POST' and route.endswith('/releases'):
        if os.environ.get('FAIL_RELEASE_CREATE'):
            sys.exit(25)
        (root / 'created').touch()
        print('29')
    elif '/actions/runs/' in route:
        print((root / 'run.json').read_text())
    elif '/releases/' in route:
        release = json.loads((root / 'release.json').read_text())
        if '--jq' not in args:
            print(json.dumps(release))
        elif args[args.index('--jq') + 1] == '.tag_name':
            print(release['tag_name'])
        else:
            values = [release['id'], release['tag_name'], release['draft'],
                      release['prerelease'], release['published_at'] is not None]
            print('\\t'.join(json.dumps(value) if not isinstance(value, str) else value
                            for value in values))
    elif '/git/ref/tags/' in route:
        print(os.environ['TAG_SHA'])
    elif route.endswith('/commits/main'):
        print(os.environ['MAIN_SHA'])
    elif '/commits/' in route:
        print(os.environ['TAG_SHA'])
    else:
        sys.exit(93)
elif args[:2] == ['release', 'view']:
    release = json.loads((root / 'release.json').read_text())
    if '--jq' in args:
        query = args[args.index('--jq') + 1]
        fields = {'.isDraft': release['draft'], '.isPrerelease': release['prerelease'],
                  '.databaseId': int(os.environ.get('TAG_RELEASE_ID', release['id']))}
        print(json.dumps(fields[query]))
    elif '--json' in args:
        print(json.dumps({'isDraft': release['draft'], 'isPrerelease': release['prerelease'],
                          'publishedAt': release['published_at'], 'databaseId': release['id']}))
elif args[:2] == ['release', 'download']:
    name = args[args.index('--pattern') + 1]
    destination = Path(args[args.index('--dir') + 1])
    counter = root / ('attempt-' + name)
    attempts = int(counter.read_text()) if counter.exists() else 0
    counter.write_text(str(attempts + 1))
    destination.mkdir(exist_ok=True)
    if os.environ.get('FLAKY_ASSET') == name and attempts == 0:
        (destination / name).write_bytes(b'first download mismatch')
    else:
        shutil.copyfile(root / 'public' / name, destination / name)
elif args[:2] in (['release', 'delete-asset'], ['release', 'edit']):
    pass
else:
    sys.exit(94)
"""

    def execute(self, name, workflow=WORKFLOW):
        (self.runner / 'run.json').write_text(json.dumps(self.run), encoding='utf-8')
        (self.runner / 'release.json').write_text(json.dumps(self.release), encoding='utf-8')
        script = """git() {
    case "$*" in
      'fetch --force --tags origin') return 0 ;;
      'fetch --no-tags --depth=1 origin main') return 0 ;;
      'tag --points-at '*) printf '%s\\n' "$FIXTURE_VERSION_TAGS" ;;
      'rev-list -n 1 '*) printf '%s\\n' "$TAG_SHA" ;;
      'rev-parse HEAD') printf '%s\\n' "$SOURCE_SHA" ;;
      'rev-parse FETCH_HEAD')
        if [ -f "$FIXTURE_RUNNER/created" ]; then
            printf '%s\\n' "${MAIN_AFTER_CREATE:-$MAIN_SHA}"
        else
            printf '%s\\n' "$MAIN_SHA"
        fi ;;
      *) return 95 ;;
    esac
}
""" + block(name, workflow)
        return subprocess.run([BASH, '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
            cwd=self.root, env=self.environment, capture_output=True, text=True, timeout=30)

    def resolve(self):
        return self.execute('Resolve matching version tag and published release')

    def manifest(self):
        download = self.root / '.release-download/native'
        download.mkdir(parents=True, exist_ok=True)
        lines = ['maris-release-v2', 'version\t1.2.3',
                 'source_sha256\t' + self.environment['SOURCE_SHA256'],
                 'channel\tstable', 'interface\tcli']
        for system, arch in TARGETS:
            name = f'Maris-1.2.3-{system}-{arch}' + ('.zip' if system == 'windows' else '.tar.gz')
            data = ('synthetic archive ' + name).encode()
            (download / name).write_bytes(data)
            lines.append('\t'.join(['asset', system, arch, name,
                hashlib.sha256(data).hexdigest(), str(len(data)), 'c' * 64]))
        path = download / 'maris-release.tsv'
        data = ('\n'.join(lines) + '\n').encode('ascii')
        path.write_bytes(data)
        return path, data

    def calls(self):
        path = self.runner / 'calls.jsonl'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_real_commit_and_distinct_source_digest_accept_valid_release(self):
        result = self.resolve()
        self.assertEqual(result.returncode, 0, result.stderr)
        output = (self.runner / 'outputs').read_text()
        self.assertIn('publish=true', output)
        self.assertRegex(output, r'(?m)^source_sha256=[a-f0-9]{64}$')
        values = dict(line.split('=', 1) for line in output.splitlines())
        self.environment['SOURCE_SHA256'] = values['source_sha256']
        path, data = self.manifest()
        result = self.execute('Verify installer contract and prepare public assets')
        self.assertEqual(result.returncode, 0, result.stderr)
        upload = self.root / '.release-upload'
        self.assertEqual(len(list(upload.iterdir())), 8)
        self.assertEqual((upload / path.name).read_bytes(), data)

    def test_automatic_stable_accepts_successful_manual_build(self):
        self.run['event'] = 'workflow_dispatch'
        result = self.resolve()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('publish=true', (self.runner / 'outputs').read_text())

    def test_automatic_without_version_tags_skips_before_github_lookup(self):
        for tags in ('', 'nightly\nv1.2\nv1.2.3-rc.1'):
            with self.subTest(tags=tags):
                self.environment['FIXTURE_VERSION_TAGS'] = tags
                output = self.runner / 'outputs'
                output.unlink(missing_ok=True)
                result = self.resolve()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(), 'publish=false\n')
                self.assertEqual(self.calls(), [])

    def test_automatic_multiple_version_tags_rejects_before_github_lookup(self):
        self.environment['FIXTURE_VERSION_TAGS'] = 'v1.2.3\nv1.2.4'
        result = self.resolve()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('Expected exactly one version tag', result.stderr)
        self.assertFalse((self.runner / 'outputs').exists())
        self.assertEqual(self.calls(), [])

    def test_wrong_run_identity_is_rejected_before_asset_work(self):
        original = dict(self.run)
        mutations = [('id', 18), ('head_sha', 'b' * 40), ('conclusion', 'failure'),
                     ('status', 'in_progress'), ('event', 'pull_request'),
                     ('path', '.github/workflows/other.yml'),
                     ('repository', {'full_name': 'other/repository'}),
                     ('head_repository', {'full_name': 'fork/maris'})]
        for key, value in mutations:
            with self.subTest(field=key):
                self.run = dict(original, **{key: value})
                result = self.resolve()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse((self.root / '.release-upload').exists())

    def test_automatic_path_rejects_drafts_prereleases_and_unpublished_releases(self):
        original = dict(self.release)
        for change in ({'draft': True}, {'prerelease': True}, {'published_at': None}):
            with self.subTest(change=change):
                self.release = dict(original, **change)
                result = self.resolve()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse(any(call[:2] == ['release', 'edit'] for call in self.calls()))

    def test_explicit_path_requires_exact_owned_draft(self):
        self.environment.update(RELEASE_TAG_INPUT='v1.2.3', RELEASE_ID_INPUT='29',
                                CALLED_FROM_PUBLISH='true',
                                FIXTURE_VERSION_TAGS='v1.2.3\nv1.2.4')
        self.release.update(draft=True, prerelease=False, published_at=None)
        result = self.resolve()
        self.assertEqual(result.returncode, 0, result.stderr)
        for change in ({'id': 30}, {'draft': False}, {'prerelease': True}):
            original = dict(self.release)
            with self.subTest(change=change):
                self.release.update(change)
                result = self.resolve()
                self.assertNotEqual(result.returncode, 0, result.stdout)
            self.release = original
        self.run['head_branch'] = 'untrusted-branch'
        self.assertNotEqual(self.resolve().returncode, 0)

    def test_manifest_trailing_line_endings_match_public_unix_installer(self):
        path, valid = self.manifest()
        installer = (ROOT / 'install.sh').read_text(encoding='utf-8')
        start = installer.index('function bad() {')
        end = installer.index('\n\' "$WORK/manifest.tsv")', start)
        awk_program = installer[start:end]
        for trailing_lf in range(4):
            with self.subTest(trailing_lf=trailing_lf):
                path.write_bytes(valid[:-1] + b'\n' * trailing_lf)
                shutil.rmtree(self.root / '.release-upload', ignore_errors=True)
                parsed = subprocess.run(
                    [BASH, '--noprofile', '--norc', '-c',
                     'awk -F "\\t" -v target_os=linux -v arch=x86_64 '
                     '-v requested=1.2.3 "$1" "$2"',
                     'installer-manifest-fixture', awk_program, path.as_posix()],
                    cwd=self.root, env=self.environment, capture_output=True,
                    text=True, timeout=30,
                )
                self.assertEqual(parsed.returncode, 0 if trailing_lf <= 1 else 2,
                                 parsed.stderr)
                result = self.execute('Verify installer contract and prepare public assets')
                self.assertEqual(
                    result.returncode == 0, parsed.returncode == 0,
                    f'Publication and installer disagreed for {trailing_lf} trailing LF: '
                    + result.stderr,
                )

    def test_wrong_source_digest_and_crlf_manifest_are_rejected(self):
        path, original = self.manifest()
        for data in (original.replace(SOURCE.encode(), b'd' * 64),
                     original.replace(SOURCE.encode(), COMMIT.encode()),
                     original.replace(b'\n', b'\r\n'),
                     original.replace(b'channel\tstable', b'channel\tcandidate'),
                     original.replace(b'interface\tcli', b'interface\tgui')):
            with self.subTest(manifest=data[:130]):
                shutil.rmtree(self.root / '.release-upload', ignore_errors=True)
                path.write_bytes(data)
                result = self.execute('Verify installer contract and prepare public assets')
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse((self.root / '.release-upload').exists())

    def test_manifest_and_archive_links_cannot_escape_downloads(self):
        path, original = self.manifest()
        outside = self.runner / 'outside.tsv'
        outside.write_bytes(original)
        path.unlink()
        try:
            path.symlink_to(outside)
        except OSError as error:
            self.skipTest(f'Symlink fixture unavailable: {error}')
        result = self.execute('Verify installer contract and prepare public assets')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / '.release-upload').exists())

        path.unlink()
        path.write_bytes(original)
        archive = next(path.parent.glob('*.zip'))
        outside_archive = self.runner / archive.name
        archive.replace(outside_archive)
        archive.symlink_to(outside_archive)
        result = self.execute('Verify installer contract and prepare public assets')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / '.release-upload' / archive.name).exists())

    def test_public_byte_reverification_retries_repair_and_rejects_persistent_mismatch(self):
        self.manifest()
        prepared = self.execute('Verify installer contract and prepare public assets')
        self.assertEqual(prepared.returncode, 0, prepared.stderr)
        shutil.copytree(self.root / '.release-upload', self.runner / 'public')
        asset = 'Maris-1.2.3-linux-arm64.tar.gz'
        self.environment['FLAKY_ASSET'] = asset
        result = self.execute('Reverify public installer asset bytes before installation smoke')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(call[:2] == ['release', 'delete-asset'] for call in self.calls()))
        self.environment.pop('FLAKY_ASSET')
        (self.runner / 'public' / asset).write_bytes(b'persistent mismatch')
        result = self.execute('Reverify public installer asset bytes before installation smoke')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        deleted = [call[3] for call in self.calls() if call[:2] == ['release', 'delete-asset']]
        self.assertEqual(deleted, [asset])

    def mutations(self):
        return [call for call in self.calls()
                if call[0] == 'api' and '--method' in call
                and call[call.index('--method') + 1] in ('POST', 'PATCH', 'DELETE')]

    def test_exposure_mutates_only_the_exact_owned_draft(self):
        name = 'Expose validated release for public smoke'
        result = self.execute(name)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.mutations(), [])
        self.environment['CALLED_FROM_PUBLISH'] = 'true'
        self.release.update(draft=True, prerelease=False, published_at=None)
        result = self.execute(name)
        self.assertEqual(result.returncode, 0, result.stderr)
        mutation, = self.mutations()
        self.assertIn('repos/fixture/maris/releases/29', mutation)
        self.assertIn('draft=false', mutation)
        self.assertIn('prerelease=true', mutation)
        self.assertIn('make_latest=false', mutation)

        (self.runner / 'calls.jsonl').unlink()
        self.environment['TAG_RELEASE_ID'] = '30'
        self.assertNotEqual(self.execute(name).returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_automatic_exposure_rejects_changed_state_without_publishing(self):
        original = dict(self.release)
        for change in ({'draft': True}, {'prerelease': True}, {'published_at': None},
                       {'id': 30}, {'tag_name': 'v9.9.9'}):
            with self.subTest(change=change):
                self.release = dict(original, **change)
                result = self.execute('Expose validated release for public smoke')
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(self.mutations(), [])

    def test_promotion_requires_main_tag_and_owned_public_prerelease(self):
        name = 'Promote smoke-tested prerelease to stable'
        self.release['prerelease'] = True
        result = self.execute(name)
        self.assertEqual(result.returncode, 0, result.stderr)
        mutation, = self.mutations()
        self.assertIn('repos/fixture/maris/releases/29', mutation)
        self.assertIn('prerelease=false', mutation)
        self.assertIn('make_latest=true', mutation)
        (self.runner / 'calls.jsonl').unlink()
        for key, value in (('MAIN_SHA', 'b' * 40), ('TAG_SHA', 'b' * 40),
                           ('TAG_RELEASE_ID', '30')):
            with self.subTest(identity=key):
                previous = self.environment.get(key)
                self.environment[key] = value
                self.assertNotEqual(self.execute(name).returncode, 0)
                self.assertEqual(self.mutations(), [])
                if previous is None:
                    self.environment.pop(key)
                else:
                    self.environment[key] = previous
        self.release['prerelease'] = False
        self.assertNotEqual(self.execute(name).returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_creation_records_release_id_and_rolls_back_only_owned_objects(self):
        name = 'Create draft GitHub release'
        result = self.execute(name, PUBLISH_WORKFLOW)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('release_id=29', (self.runner / 'outputs').read_text())
        self.assertEqual([call[call.index('--method') + 1] for call in self.mutations()],
                         ['POST', 'POST'])
        (self.runner / 'calls.jsonl').unlink()
        (self.runner / 'created').unlink()
        self.environment['MAIN_AFTER_CREATE'] = 'b' * 40
        result = self.execute(name, PUBLISH_WORKFLOW)
        self.assertNotEqual(result.returncode, 0)
        deletes = [call for call in self.mutations() if 'DELETE' in call]
        self.assertEqual([next(value for value in call if value.startswith('repos/'))
                          for call in deletes],
                         ['repos/fixture/maris/releases/29',
                          'repos/fixture/maris/git/refs/tags/v1.2.3'])

    def test_failed_creation_preserves_existing_or_unconfirmed_objects(self):
        name = 'Create draft GitHub release'
        for flag in ('FAIL_TAG_CREATE', 'FAIL_RELEASE_CREATE'):
            with self.subTest(failure=flag):
                self.environment[flag] = '1'
                result = self.execute(name, PUBLISH_WORKFLOW)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any('DELETE' in call for call in self.mutations()))
                self.environment.pop(flag)
        self.environment['TAG_SHA'] = 'b' * 40
        result = self.execute(name, PUBLISH_WORKFLOW)
        self.assertNotEqual(result.returncode, 0)
        deletes = [call for call in self.mutations() if 'DELETE' in call]
        self.assertEqual(len(deletes), 1)
        self.assertIn('repos/fixture/maris/releases/29', deletes[0])

    def test_asset_failure_rollback_preserves_moved_tag_and_deletes_recorded_id(self):
        name = 'Remove release and tag after asset publication failure'
        self.environment['TAG_RELEASE_ID'] = '30'
        self.environment['TAG_SHA'] = 'b' * 40
        result = self.execute(name, PUBLISH_WORKFLOW)
        self.assertNotEqual(result.returncode, 0)
        mutation, = self.mutations()
        self.assertEqual(mutation, ['api', '--method', 'DELETE',
                                   'repos/fixture/maris/releases/29'])
        (self.runner / 'calls.jsonl').unlink()
        self.environment['TAG_SHA'] = COMMIT
        result = self.execute(name, PUBLISH_WORKFLOW)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.mutations()), 2)
        self.assertIn('repos/fixture/maris/git/refs/tags/v1.2.3', self.mutations()[1])


if __name__ == '__main__':
    unittest.main()

