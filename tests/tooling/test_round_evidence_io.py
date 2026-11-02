"""Adversarial verification evidence fixtures; never execute fixture payloads."""
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import verify_rounds


class RoundEvidenceIO(unittest.TestCase):
    def test_regular_evidence_accepts_exact_bound_and_empty_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'evidence.log'
            path.write_bytes(b'abcd')
            self.assertEqual(verify_rounds.read_owned(path, 4), b'abcd')
            with self.assertRaises(ValueError):
                verify_rounds.read_owned(path, 3)
            path.write_bytes(b'')
            self.assertEqual(verify_rounds.read_owned(path, 0), b'')

    def test_evidence_never_uses_unbounded_path_read_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'evidence.log'
            path.write_bytes(b'abcd')
            with mock.patch.object(Path, 'read_bytes', side_effect=AssertionError('unbounded evidence read')):
                self.assertEqual(verify_rounds.read_owned(path, 4), b'abcd')

    def test_replacement_or_growth_between_metadata_and_open_is_rejected(self):
        for replacement_data in (b'evil', b'x' * 4096):
            with self.subTest(size=len(replacement_data)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                path = root / 'evidence.log'
                path.write_bytes(b'good')
                replacement = root / 'replacement'
                replacement.write_bytes(replacement_data)
                original_io_open, original_os_open = io.open, os.open
                replaced = False

                def swap(file):
                    nonlocal replaced
                    if not replaced and isinstance(file, (str, os.PathLike)) and Path(file) == path:
                        replaced = True
                        os.replace(replacement, path)

                def io_open(file, *args, **kwargs):
                    swap(file)
                    return original_io_open(file, *args, **kwargs)

                def os_open(file, *args, **kwargs):
                    swap(file)
                    return original_os_open(file, *args, **kwargs)

                with mock.patch('io.open', side_effect=io_open), mock.patch('os.open', side_effect=os_open):
                    with self.assertRaises(ValueError):
                        verify_rounds.read_owned(path, 4)
                self.assertTrue(replaced, 'the race injection must actually run')

    @unittest.skipIf(sys.platform == 'win32', 'POSIX symlink fixture')
    def test_linked_or_nonregular_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / 'target'
            target.write_bytes(b'ok')
            link = root / 'evidence.log'
            link.symlink_to(target)
            with self.assertRaises(ValueError):
                verify_rounds.read_owned(link, 16)
            with self.assertRaises(ValueError):
                verify_rounds.read_owned(root, 16)

    def test_mutation_during_read_is_rejected_and_the_read_request_stays_bounded(self):
        modes = ['grow-before-read', 'grow-after-read', 'rewrite-after-read']
        if sys.platform != 'win32':
            modes.append('replace-after-read')
        for mode in modes:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                path = root / 'evidence.log'
                path.write_bytes(b'good')
                replacement = root / 'replacement'
                replacement.write_bytes(b'evil')
                original_fdopen = os.fdopen
                read_sizes = []
                changed = False

                def mutate():
                    nonlocal changed
                    if mode.startswith('grow'):
                        with path.open('ab') as output:
                            output.write(b'x' * 4096)
                    elif mode.startswith('replace'):
                        os.replace(replacement, path)
                    else:
                        before = path.stat()
                        path.write_bytes(b'evil')
                        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns + 5_000_000_000))
                    changed = True

                class Reader:
                    def __init__(self, file):
                        self.file = file

                    def __enter__(self):
                        return self

                    def __exit__(self, *args):
                        return self.file.__exit__(*args)

                    def fileno(self):
                        return self.file.fileno()

                    def read(self, size=-1):
                        read_sizes.append(size)
                        if mode == 'grow-before-read':
                            mutate()
                        data = self.file.read(size)
                        if mode != 'grow-before-read':
                            mutate()
                        return data

                with mock.patch('os.fdopen', side_effect=lambda *args: Reader(original_fdopen(*args))):
                    with self.assertRaises(ValueError):
                        verify_rounds.read_owned(path, 4)
                self.assertTrue(changed, 'the mutation must actually occur')
                self.assertEqual(read_sizes, [5], 'the read must be limited to bound + one sentinel byte')

    def test_resume_rejects_boolean_and_float_exit_codes_in_retained_json(self):
        from test_round_resume import fixture
        for code in (False, 0.0):
            with self.subTest(code=repr(code)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                output, checkpoint, record = fixture(root)
                record['steps'][0]['exit_code'] = code
                (output / 'rounds.jsonl').write_text(json.dumps(record) + '\n', encoding='utf-8')
                with self.assertRaises(ValueError):
                    verify_rounds.resume_records(output, checkpoint, 'a' * 64, root)

    def test_success_requires_an_actual_integer_process_exit_code(self):
        data = b'test result: ok. 1 passed; 0 failed; 0 ignored;\n'
        self.assertTrue(verify_rounds.test_passed(data, 0))
        for code in (False, 0.0, None, '0', 1):
            with self.subTest(code=repr(code)):
                self.assertFalse(verify_rounds.test_passed(data, code))


if __name__ == '__main__':
    unittest.main()
