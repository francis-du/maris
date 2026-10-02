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
