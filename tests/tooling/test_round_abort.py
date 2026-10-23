"""Abort/consumer regressions; isolated log fixtures never execute test binaries."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import release_gate
import verify_rounds
from test_round_resume import fixture

LOG = b'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'


class RoundAbort(unittest.TestCase):
    @contextlib.contextmanager
    def environment(self):
        with tempfile.TemporaryDirectory() as temporary, contextlib.ExitStack() as patches:
            root = Path(temporary).resolve()
            output, checkpoint, _ = fixture(root)
            destination = verify_rounds.verification_root(root) / 'abort-fixture'
            output.rename(destination)
            output = destination
            checkpoint['requested_rounds'] = 3
            (output / 'run.json').write_text(json.dumps(checkpoint), encoding='utf-8')
            (output / 'report.json').write_text('{"passed":false,"complete":false}\n', encoding='utf-8')
            launched = []
            original_checkpoint = verify_rounds.checkpoint_binaries

            def launch(command, **_kwargs):
                launched.append(Path(command[0]).name)
                return subprocess.CompletedProcess(command, 0, LOG, b'')

            patches.enter_context(mock.patch.object(verify_rounds, 'ROOT', root))
            patches.enter_context(mock.patch.object(verify_rounds, 'source_digest', return_value='a' * 64))
            patches.enter_context(mock.patch.object(verify_rounds, 'checkpoint_binaries',
                side_effect=lambda paths, *_args: original_checkpoint(paths, root)))
            patches.enter_context(mock.patch.object(verify_rounds.subprocess, 'run', side_effect=launch))
            patches.enter_context(mock.patch.object(verify_rounds, 'compile_binaries',
                side_effect=AssertionError('resume fixture must not compile or run real binaries')))
            patches.enter_context(mock.patch.object(sys, 'argv', [
                'verify_rounds.py', '--resume', str(output), '--batch-size', '1', '--jobs', '1']))
            patches.enter_context(contextlib.redirect_stdout(io.StringIO()))
            yield output, launched

    def assert_abort_retained(self, output, error):
        with self.assertRaises(type(error)) as raised:
            verify_rounds.main()
        self.assertIs(raised.exception, error, 'cleanup must preserve the original failure')
        lock = output / '.batch.lock'
        self.assertTrue(lock.is_file(), 'aborted batches must retain their lock, not silently retry')
        self.assertTrue(lock.read_text(encoding='ascii').isdigit())
        with self.assertRaises(FileExistsError):
            verify_rounds.main()

    def test_process_start_errors_and_interrupts_retain_the_batch_lock(self):
        for error in [OSError('spawn failed'), RuntimeError('executor failed'),
                      KeyboardInterrupt('interrupted'), SystemExit('terminated')]:
            with self.subTest(error=type(error).__name__), self.environment() as (output, _):
                with mock.patch.object(verify_rounds.subprocess, 'run', side_effect=error):
                    self.assert_abort_retained(output, error)
                self.assertEqual(len((output / 'rounds.jsonl').read_text().splitlines()), 1)

    def test_log_write_error_cannot_be_retried_as_an_unexecuted_round(self):
        with self.environment() as (output, launched):
            error = OSError('log storage unavailable')
            original = Path.write_bytes

            def write(path, data):
                if path.name == 'round-002-release_matrix.log':
                    raise error
                return original(path, data)

            with mock.patch.object(Path, 'write_bytes', write):
                self.assert_abort_retained(output, error)
            self.assertEqual(launched, ['release_matrix'])

    def test_record_append_error_preserves_executed_logs_and_lock(self):
        with self.environment() as (output, launched):
            error = OSError('record append unavailable')
            original = Path.open

            def open_file(path, mode='r', *args, **kwargs):
                if path.name == 'rounds.jsonl' and mode == 'a':
                    raise error
                return original(path, mode, *args, **kwargs)

            with mock.patch.object(Path, 'open', open_file):
                self.assert_abort_retained(output, error)
            self.assertEqual(len(launched), 2)
            self.assertTrue((output / 'round-002-release_matrix.log').is_file())

    def test_final_source_check_error_retains_the_lock(self):
        with self.environment() as (output, launched):
            error = OSError('source snapshot unavailable')
            with mock.patch.object(verify_rounds, 'source_digest', side_effect=['a' * 64, 'a' * 64, error, 'a' * 64]):
                self.assert_abort_retained(output, error)
            self.assertEqual(len(launched), 2)

    def test_report_write_failure_never_unlocks_an_unsettled_batch(self):
        with self.environment() as (output, launched):
            error = OSError('report storage unavailable')
            original = Path.write_text

            def write(path, *args, **kwargs):
                if path.name == 'report.json':
                    raise error
                return original(path, *args, **kwargs)

            with mock.patch.object(Path, 'write_text', write):
                self.assert_abort_retained(output, error)
            self.assertEqual(len(launched), 2)
            self.assertFalse(json.loads((output / 'report.json').read_text())['passed'])

    def test_successful_partial_batches_still_unlock_and_resume_without_reexecution(self):
        with self.environment() as (output, launched):
            for complete in [False, True, True]:
                self.assertEqual(verify_rounds.main(), 0)
                self.assertFalse((output / '.batch.lock').exists())
                report = json.loads((output / 'report.json').read_text())
                self.assertEqual(report['complete'], complete)
                self.assertEqual(report['passed'], complete)
            self.assertEqual(launched, ['release_matrix', verify_rounds.SUITES[1],
                                       'release_matrix', verify_rounds.SUITES[2]])

    def test_reported_test_failure_unlocks_only_after_retaining_failure_evidence(self):
        with self.environment() as (output, _):
            with mock.patch.object(verify_rounds.subprocess, 'run', return_value=
                    subprocess.CompletedProcess(['fixture'], 1, b'test result: FAILED.\n', b'')):
                self.assertEqual(verify_rounds.main(), 1)
            self.assertFalse((output / '.batch.lock').exists())
            self.assertTrue((output / 'invalidated.json').is_file())
            self.assertFalse(json.loads((output / 'report.json').read_text())['passed'])

    def test_summary_output_failure_keeps_a_completed_report_locked(self):
        with self.environment() as (output, launched):
            self.assertEqual(verify_rounds.main(), 0)
            error = BrokenPipeError('summary pipe closed')
            with mock.patch('builtins.print', side_effect=error):
                self.assert_abort_retained(output, error)
            report = json.loads((output / 'report.json').read_text())
            self.assertTrue(report['complete'] and report['passed'])
            self.assertEqual(len(launched), 4)

    def test_aborted_parallel_batch_preserves_peer_logs_without_retry(self):
        with self.environment() as (output, _):
            error = OSError('one parallel round failed to start')

            def launch(command, **kwargs):
                if kwargs['env']['MARIS_CHECK_SEED'] == '2':
                    raise error
                self.assertTrue((output / '.batch.lock').is_file())
                return subprocess.CompletedProcess(command, 0, LOG, b'')

            arguments = ['verify_rounds.py', '--resume', str(output), '--batch-size', '2', '--jobs', '2']
            with mock.patch.object(sys, 'argv', arguments), \
                    mock.patch.object(verify_rounds.subprocess, 'run', side_effect=launch):
                self.assert_abort_retained(output, error)
            self.assertTrue((output / 'round-003-release_matrix.log').is_file())
            self.assertEqual(len((output / 'rounds.jsonl').read_text().splitlines()), 1)

    def test_an_existing_lock_is_not_removed_or_overwritten(self):
        with self.environment() as (output, launched):
            lock = output / '.batch.lock'
            lock.write_text('owned-by-another-batch', encoding='ascii')
            with self.assertRaises(FileExistsError):
                verify_rounds.main()
            self.assertEqual(lock.read_text(), 'owned-by-another-batch')
            self.assertEqual(launched, [])


def release_fixture(root):
    records = []
    for index in range(1, 201):
        steps = []
        for suite in ['release_matrix', verify_rounds.SUITES[(index - 1) % len(verify_rounds.SUITES)]]:
            name = f'round-{index:03d}-{suite}.log'
            (root / name).write_bytes(LOG)
            steps.append({'suite': suite, 'log': name, 'exit_code': 0, 'passed': True,
                          'log_sha256': hashlib.sha256(LOG).hexdigest()})
        records.append({'round': index, 'seed': index, 'passed': True, 'steps': steps})
    return {'source_sha256': 'fixture', 'source_unchanged': True, 'binaries_unchanged': True,
            'passed': True, 'complete': True, 'completed_rounds': 200,
            'requested_rounds': 200, 'passed_rounds': 200, 'records': records}


class ReleaseBatchState(unittest.TestCase):
    def test_active_or_interrupted_batch_cannot_supply_release_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = release_fixture(root)
            release_gate.check_report(report, 'fixture', root)
            (root / '.batch.lock').write_text('12345', encoding='ascii')
            with self.assertRaises(ValueError):
                release_gate.check_report(report, 'fixture', root)

    def test_marker_metadata_errors_are_not_treated_as_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = release_fixture(root)
            original = Path.lstat

            def lstat(path, *args, **kwargs):
                if path.name == '.batch.lock':
                    raise PermissionError('marker metadata unavailable')
                return original(path, *args, **kwargs)

            with mock.patch.object(Path, 'lstat', lstat), self.assertRaises(PermissionError):
                release_gate.check_report(report, 'fixture', root)

    def test_batch_state_is_rechecked_after_all_logs_are_read(self):
        for name in ['.batch.lock', 'invalidated.json']:
            with self.subTest(marker=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                report = release_fixture(root)
                original = release_gate.read_owned
                reads = 0

                def read(path, bound):
                    nonlocal reads
                    data = original(path, bound)
                    reads += 1
                    if reads == 200:
                        (root / name).write_text('interrupted during validation', encoding='utf-8')
                    return data

                with mock.patch.object(release_gate, 'read_owned', side_effect=read):
                    with self.assertRaises(ValueError):
                        release_gate.check_report(report, 'fixture', root)
                self.assertGreaterEqual(reads, 200, 'fault injection must actually happen')


if __name__ == '__main__':
    unittest.main()
