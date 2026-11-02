"""Checkpoint integrity fixtures, not test-run evidence and not executable payloads."""
from pathlib import Path
import hashlib
import json
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from verify_rounds import SUITES, checkpoint_binaries, resume_directory, resume_records, verification_root


def fixture(root: Path):
    target = root / 'target'; target.mkdir()
    binaries = {}
    for name in ['release_matrix'] + SUITES:
        path = target / name; path.write_bytes(b'non-executable checksum fixture')
        binaries[name] = str(path)
    output = root / 'evidence'; output.mkdir()
    checkpoint = {'schema_version': 1, 'source_sha256': 'a' * 64, 'requested_rounds': 200,
                  'binaries': checkpoint_binaries(binaries, root)}
    record = {'round': 1, 'seed': 1, 'passed': True, 'steps': []}
    data = b'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
    for suite in ['release_matrix', SUITES[0]]:
        name = f'round-001-{suite}.log'; (output / name).write_bytes(data)
        record['steps'].append({'suite': suite, 'exit_code': 0, 'passed': True,
                                'log': name, 'log_sha256': hashlib.sha256(data).hexdigest()})
    (output / 'rounds.jsonl').write_text(json.dumps(record) + '\n')
    return output, checkpoint, record


class ResumeIntegrity(unittest.TestCase):
    @unittest.skipIf(sys.platform == 'win32', 'POSIX symlink fixtures complement cross-platform reparse tests')
    def test_verification_root_and_resume_reject_linked_directories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real_review = root / 'real-review'
            (real_review / 'verification').mkdir(parents=True)
            (root / '.maris-review').symlink_to(real_review, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'Linked verification directory'):
                verification_root(root)
            (root / '.maris-review').unlink()
            verification = verification_root(root)
            real_run = verification / 'real-run'
            real_run.mkdir()
            linked_run = verification / 'linked-run'
            linked_run.symlink_to(real_run, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'Resume must name'):
                resume_directory(linked_run, verification)

    @unittest.skipIf(sys.platform == 'win32', 'POSIX symlink fixture complements cross-platform reparse tests')
    def test_checkpoint_rejects_linked_target_and_binary_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            real_target = root / 'real-target'
            real_target.mkdir()
            (root / 'target').symlink_to(real_target, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'compiled target directory'):
                checkpoint_binaries({}, root)

    def test_only_retained_complete_passing_rounds_can_be_reused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); output, checkpoint, record = fixture(root)
            binaries, records = resume_records(output, checkpoint, 'a' * 64, root)
            self.assertEqual(records, [record])
            self.assertEqual(set(binaries), set(SUITES + ['release_matrix']))
            self.assertLess(len(records), checkpoint['requested_rounds'])

    def test_source_or_compiled_binary_changes_reject_resume(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); output, checkpoint, _ = fixture(root)
            with self.assertRaisesRegex(ValueError, 'Source changed'):
                resume_records(output, checkpoint, 'b' * 64, root)
            Path(checkpoint['binaries']['release_matrix']['path']).write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'Compiled binaries changed'):
                resume_records(output, checkpoint, 'a' * 64, root)

    def test_modified_logs_reject_resume_even_when_summary_says_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); output, checkpoint, record = fixture(root)
            (output / record['steps'][0]['log']).write_text('different output')
            with self.assertRaises(ValueError):
                resume_records(output, checkpoint, 'a' * 64, root)

    def test_failed_or_incomplete_round_is_not_retried_as_a_successful_checkpoint(self):
        for change in [{'passed': False}, {'round': 2}, {'seed': 2}, {'steps': []}]:
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve(); output, checkpoint, record = fixture(root)
                record.update(change); (output / 'rounds.jsonl').write_text(json.dumps(record) + '\n')
                with self.assertRaises(ValueError):
                    resume_records(output, checkpoint, 'a' * 64, root)

    def test_duplicate_seed_and_log_path_escape_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); output, checkpoint, record = fixture(root)
            (output / 'rounds.jsonl').write_text((json.dumps(record) + '\n') * 2)
            with self.assertRaises(ValueError):
                resume_records(output, checkpoint, 'a' * 64, root)
            record['steps'][0]['log'] = '../escape'
            (output / 'rounds.jsonl').write_text(json.dumps(record) + '\n')
            with self.assertRaises(ValueError):
                resume_records(output, checkpoint, 'a' * 64, root)

    def test_zero_test_log_cannot_become_pass_by_rehashing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); output, checkpoint, record = fixture(root)
            data = b'test result: ok. 0 passed; 0 failed; 0 ignored;\n'
            (output / record['steps'][0]['log']).write_bytes(data)
            record['steps'][0]['log_sha256'] = hashlib.sha256(data).hexdigest()
            (output / 'rounds.jsonl').write_text(json.dumps(record) + '\n')
            with self.assertRaises(ValueError):
                resume_records(output, checkpoint, 'a' * 64, root)


if __name__ == '__main__':
    unittest.main()
