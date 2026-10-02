"""Source publication checks use disposable Git repositories, not the user's index."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import source_audit


class SourceHygiene(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.git('init', '--quiet')
        self.git('config', 'core.autocrlf', 'false')
        (self.root / 'README.md').write_text('# Isolated source fixture\n', encoding='utf-8')

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, capture_output=True, check=True).stdout

    def test_retired_compile_launchers_are_not_publishable_entry_points(self):
        for name in ('run.sh', 'run.ps1'):
            with self.subTest(name=name):
                path = self.root / name
                path.write_text('retired development wrapper\n', encoding='utf-8')
                with self.assertRaisesRegex(ValueError, 'Unexpected publish path'):
                    source_audit.source_files(self.root)
                path.unlink()
        self.assertFalse((ROOT / 'run.sh').exists())
        self.assertFalse((ROOT / 'run.ps1').exists())

    def test_vendored_flexaudio_core_is_audited_for_credentials(self):
        vendor = self.root / 'third_party/flexaudio-core'
        (vendor / 'src').mkdir(parents=True)
        (vendor / 'Cargo.toml').write_text('[package]\nname = "flexaudio-core"\n', encoding='utf-8')
        source = vendor / 'src/lib.rs'
        source.write_text('// Audited vendored source\n', encoding='utf-8')
        names = {p.relative_to(self.root).as_posix() for p in source_audit.source_files(self.root)}
        self.assertIn('third_party/flexaudio-core/Cargo.toml', names)
        self.assertIn('third_party/flexaudio-core/src/lib.rs', names)
        credential = '-----BEGIN ' + 'PRIVATE KEY-----'
        source.write_text(credential + '\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Potential credential') as failure:
            source_audit.source_files(self.root)
        self.assertNotIn(credential, str(failure.exception))

    def test_unreviewed_third_party_packages_are_not_publishable(self):
        vendor = self.root / 'third_party/unreviewed/src'
        vendor.mkdir(parents=True)
        (vendor / 'lib.rs').write_text('// Unreviewed source\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Unexpected publish path'):
            source_audit.source_files(self.root)

    @unittest.skipIf(sys.platform == 'win32', 'Native directory-link fixture')
    def test_ignored_vendor_parent_links_stay_inside_the_audit_root(self):
        source = self.root / 'third_party/flexaudio-core/src/lib.rs'
        source.parent.mkdir(parents=True)
        source.write_text('// Reviewed source\n', encoding='utf-8')
        self.git('add', '--', source.relative_to(self.root).as_posix())
        source.unlink()
        source.parent.rmdir()
        # Git still lists the tracked leaf, while this rule hides its new parent link.
        (self.root / '.gitignore').write_text('/third_party/flexaudio-core/src\n', encoding='utf-8')
        outside_directory = tempfile.TemporaryDirectory()
        self.addCleanup(outside_directory.cleanup)
        outside = Path(outside_directory.name).resolve()
        (outside / 'lib.rs').write_text('// Outside the reviewed repository\n', encoding='utf-8')
        source.parent.symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'Forbidden publish input'):
            source_audit.source_files(self.root)

        source.parent.unlink()
        inside = self.root / 'third_party/musicnn/src'
        inside.mkdir(parents=True)
        (inside / 'lib.rs').write_text('// Inside the reviewed repository\n', encoding='utf-8')
        source.parent.symlink_to(inside, target_is_directory=True)
        files = source_audit.source_files(self.root)
        self.assertIn(source, files)
        self.assertTrue(all(path.resolve().is_relative_to(self.root) for path in files))

    @unittest.skipIf(sys.platform == 'win32', 'Native directory-link fixture')
    def test_audit_root_alias_preserves_staged_relative_paths(self):
        self.git('add', '--', 'README.md')
        alias_directory = tempfile.TemporaryDirectory()
        self.addCleanup(alias_directory.cleanup)
        alias = Path(alias_directory.name) / 'checkout'
        alias.symlink_to(self.root, target_is_directory=True)
        files = source_audit.source_files(alias)
        self.assertTrue(all(path.is_relative_to(alias) for path in files))
        self.assertEqual(source_audit.source_digest(alias), source_audit.source_digest(self.root))
        source_audit.check_staged(alias)

    @unittest.skipIf(sys.platform == 'win32', 'Native directory-link fixture')
    def test_cyclic_vendor_parent_links_are_explicitly_rejected(self):
        source = self.root / 'third_party/flexaudio-core/src/lib.rs'
        source.parent.mkdir(parents=True)
        source.write_text('// Reviewed source\n', encoding='utf-8')
        self.git('add', '--', source.relative_to(self.root).as_posix())
        source.unlink()
        source.parent.rmdir()
        (self.root / '.gitignore').write_text('/third_party/flexaudio-core/src\n', encoding='utf-8')
        source.parent.symlink_to('src', target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'Unresolvable publish input'):
            source_audit.source_files(self.root)

    def test_local_agent_state_is_ignored_but_design_contracts_remain_publishable(self):
        shutil.copyfile(ROOT / '.gitignore', self.root / '.gitignore')
        design = self.root / '.wcode/design'
        design.mkdir(parents=True)
        (self.root / '.wcode/project.yaml').write_text('name: Fixture\n', encoding='utf-8')
        (design / 'product.yaml').write_text('name: Fixture\n', encoding='utf-8')
        (self.root / '.wcode/execution.json').write_text('local task state\n', encoding='utf-8')
        (self.root / '.wcode/local.log').write_text('local log\n', encoding='utf-8')
        names = {p.relative_to(self.root).as_posix() for p in source_audit.source_files(self.root)}
        self.assertIn('.wcode/project.yaml', names)
        self.assertIn('.wcode/design/product.yaml', names)
        self.assertNotIn('.wcode/execution.json', names)
        self.assertNotIn('.wcode/local.log', names)

    def test_staged_source_must_match_the_reviewed_worktree(self):
        self.git('add', '--', 'README.md')
        source_audit.check_staged(self.root)
        (self.root / 'README.md').write_text('# Changed after staging\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Staged content differs'):
            source_audit.check_staged(self.root)

    def test_unstaged_source_cannot_silently_disappear_from_the_first_commit(self):
        self.git('add', '--', 'README.md')
        (self.root / 'src').mkdir()
        (self.root / 'src/lib.rs').write_text('// Source fixture\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Staged source set differs'):
            source_audit.check_staged(self.root)

    def test_forced_generated_files_are_rejected_even_when_gitignored(self):
        (self.root / '.gitignore').write_text('/dist/\n', encoding='utf-8')
        (self.root / 'dist').mkdir()
        (self.root / 'dist/fixture.txt').write_text('generated fixture\n', encoding='utf-8')
        self.git('add', '--force', '--', 'dist/fixture.txt')
        with self.assertRaisesRegex(ValueError, 'Unexpected publish path'):
            source_audit.check_staged(self.root)

    def test_unmerged_index_is_not_a_publishable_snapshot(self):
        # No branch or working-tree mutation in the real repository.
        self.git('add', '--', 'README.md')
        blob = self.git('rev-parse', ':README.md').decode().strip()
        self.git('update-index', '--force-remove', '--', 'README.md')
        subprocess.run(['git', 'update-index', '--index-info'], cwd=self.root,
                       input=f'100644 {blob} 1\tREADME.md\n'.encode(), check=True, capture_output=True)
        with self.assertRaisesRegex(ValueError, 'Unmerged'):
            source_audit.check_staged(self.root)


if __name__ == '__main__':
    unittest.main()
