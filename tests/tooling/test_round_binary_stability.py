"""Exercise the actual round orchestrator with non-executable, isolated checksum fixtures."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import verify_rounds
from test_round_resume import fixture


class RoundBinaryStability(unittest.TestCase):
    def run_fixture(self, mutation_round=None, resume=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            _, checkpoint, _ = fixture(root)
            binaries = {name: item['path'] for name, item in checkpoint['binaries'].items()}
            original_checkpoint = verify_rounds.checkpoint_binaries
            invoked = []

            def round_stub(index, paths, output):
                invoked.append(index)
                if index == mutation_round:
                    # The source digest stays unchanged, as with a concurrent cargo rebuild.
                    Path(paths['release_matrix']).write_bytes(b'changed compiled binary fixture')
                return {'round': index, 'seed': index, 'passed': True, 'steps': []}

            arguments = ['verify_rounds.py', '--rounds', '3', '--jobs', '1']
            if resume:
                output = verify_rounds.verification_root(root) / 'resume'
                output.mkdir()
                checkpoint['requested_rounds'] = 3
                (output / 'run.json').write_text(json.dumps(checkpoint), encoding='utf-8')
                (output / 'rounds.jsonl').write_bytes(b'')
                arguments += ['--resume', str(output)]
            with mock.patch.object(verify_rounds, 'ROOT', root), \
                    mock.patch.object(verify_rounds, 'source_digest', return_value='a' * 64), \
                    mock.patch.object(verify_rounds, 'compile_binaries', return_value=binaries), \
                    mock.patch.object(verify_rounds, 'checkpoint_binaries', side_effect=lambda paths, *args: original_checkpoint(paths, root)), \
                    mock.patch.object(verify_rounds, 'run_round', side_effect=round_stub), \
                    mock.patch.object(verify_rounds.subprocess, 'run', side_effect=AssertionError('fixture must never execute a binary')), \
                    mock.patch.object(sys, 'argv', arguments), contextlib.redirect_stdout(io.StringIO()):
                result = verify_rounds.main()
            reports = list((root / '.maris-review' / 'verification').glob('*/report.json'))
            self.assertEqual(len(reports), 1)
            report = json.loads(reports[0].read_text(encoding='utf-8'))
            self.assertFalse((reports[0].parent / '.batch.lock').exists())
            self.assertEqual((reports[0].parent / 'invalidated.json').exists(), result != 0)
            self.assertEqual(report['binaries_unchanged'], mutation_round is None)
            return result, report, invoked

    def test_invalidated_rounds_cannot_resume_after_original_binaries_are_restored(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            output, checkpoint, _ = fixture(root)
            (output / 'invalidated.json').write_text('{"reason":"binary drift"}', encoding='utf-8')
            # Every file/hash and passing log is otherwise valid and unchanged.
            with self.assertRaisesRegex(ValueError, 'Invalidated verification'):
                verify_rounds.resume_records(output, checkpoint, 'a' * 64, root)

    def test_unchanged_binaries_complete_normally(self):
        code, report, invoked = self.run_fixture()
        self.assertEqual(code, 0)
        self.assertTrue(report['passed'])
        self.assertEqual(invoked, [1, 2, 3])

    def test_recompile_during_first_round_cannot_be_reported_as_success(self):
        code, report, invoked = self.run_fixture(mutation_round=1)
        self.assertNotEqual(code, 0)
        self.assertFalse(report['passed'])
        self.assertEqual(invoked, [1], 'do not launch another batch after binary drift')
        self.assertTrue(report['source_unchanged'])

    def test_recompile_during_final_round_invalidates_a_complete_run(self):
        code, report, invoked = self.run_fixture(mutation_round=3)
        self.assertNotEqual(code, 0)
        self.assertFalse(report['passed'])
        self.assertTrue(report['complete'])
        self.assertEqual(invoked, [1, 2, 3])

    def test_resume_checks_binaries_after_the_initial_checkpoint_too(self):
        code, report, invoked = self.run_fixture(mutation_round=1, resume=True)
        self.assertNotEqual(code, 0)
        self.assertFalse(report['passed'])
        self.assertEqual(invoked, [1])


if __name__ == '__main__':
    unittest.main()
