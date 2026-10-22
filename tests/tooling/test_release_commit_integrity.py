"""Commit-boundary integrity fixtures: real files/hard links, never executable payloads."""
import hashlib
import inspect
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import release_bundle


class ReleaseCommitIntegrity(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='maris-commit-integrity-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.packed = self.root / 'prepared.archive'
        self.ready = self.root / 'prepared.json'
        self.archive = self.root / 'final.archive'
        self.record = self.root / 'final.release.json'
        self.payload = b'non-executable reviewed archive\n'
        self.metadata = b'{"reviewed":"original"}\n'
        self.packed.write_bytes(self.payload)
        self.ready.write_bytes(self.metadata)
        self.expected = hashlib.sha256(self.payload).hexdigest()

    def commit(self):
        release_bundle.commit_release_inputs(
            self.packed, self.ready, self.archive, self.record, self.expected,
        )

    def test_wrong_prepared_archive_digest_is_rejected_before_any_links(self):
        self.packed.write_bytes(b'x' * len(self.payload))
        with patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaises(ValueError):
                self.commit()
        link.assert_not_called()
        self.assertFalse(self.archive.exists())
        self.assertFalse(self.record.exists())

    def test_prepared_archive_replaced_at_link_is_not_reported_success(self):
        replacement = self.root / 'replacement.archive'
        replacement.write_bytes(b'x' * len(self.payload))
        real_link = os.link
        injected = False

        def interleave(source, destination):
            nonlocal injected
            if destination == self.archive:
                os.replace(replacement, self.packed)
                injected = True
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(injected, 'the source replacement must actually happen')
        self.assertFalse(self.record.exists(), 'a changed archive acquired a descriptor')

    def test_prepared_record_replaced_at_link_is_not_reported_success(self):
        replacement = self.root / 'replacement.json'
        replacement.write_bytes(b'x' * len(self.metadata))
        real_link = os.link
        injected = False

        def interleave(source, destination):
            nonlocal injected
            if destination == self.record:
                os.replace(replacement, self.ready)
                injected = True
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(injected)
        self.assertEqual(self.archive.read_bytes(), self.payload, 'do not destructively roll back')

    def test_archive_changes_during_record_commit_is_rejected(self):
        real_link = os.link
        injected = False

        def interleave(source, destination):
            nonlocal injected
            if destination == self.record:
                self.archive.write_bytes(b'x' * len(self.payload))
                injected = True
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(injected)
        self.assertTrue(self.archive.exists(), 'preserve shared paths for inspection')

    def test_record_mutation_after_link_cannot_change_the_comparison_baseline(self):
        real_link = os.link
        injected = False

        def interleave(source, destination):
            nonlocal injected
            result = real_link(source, destination)
            if destination == self.record:
                self.record.write_bytes(b'x' * len(self.metadata))
                injected = True
            return result

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(injected)
        self.assertEqual(self.ready.read_bytes(), self.record.read_bytes(),
                         'the fixture must mutate both hard-linked paths')
        self.assertNotEqual(self.record.read_bytes(), self.metadata)
        self.assertEqual(self.archive.read_bytes(), self.payload)

    def test_oversized_prepared_record_is_rejected_before_any_links(self):
        self.ready.write_bytes(b'x' * 32769)
        with patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaises(ValueError):
                self.commit()
        link.assert_not_called()
        self.assertFalse(self.archive.exists())
        self.assertFalse(self.record.exists())

    @unittest.skipIf(os.name == 'nt', 'POSIX symlink creation fixture')
    def test_linked_prepared_inputs_are_rejected_before_publication(self):
        for name in ('packed', 'ready'):
            with self.subTest(input=name):
                path = getattr(self, name)
                original = path.read_bytes()
                target = self.root / (name + '.target')
                target.write_bytes(original)
                path.unlink()
                path.symlink_to(target)
                try:
                    with patch('release_bundle.os.link', wraps=os.link) as link:
                        with self.assertRaises(ValueError):
                            self.commit()
                    link.assert_not_called()
                finally:
                    path.unlink()
                    path.write_bytes(original)
                    for output in (self.record, self.archive):
                        output.unlink(missing_ok=True)

    def test_valid_retry_never_uses_unbounded_path_read_bytes(self):
        self.commit()
        before = (self.archive.stat().st_ino, self.record.stat().st_ino)
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('unbounded commit read')):
            self.commit()
        self.assertEqual((self.archive.stat().st_ino, self.record.stat().st_ino), before)
        self.assertEqual(self.archive.read_bytes(), self.payload)
        self.assertEqual(self.record.read_bytes(), self.metadata)

    def test_exact_record_bound_is_accepted_and_empty_inputs_are_rejected(self):
        for name in ('packed', 'ready'):
            with self.subTest(empty=name):
                path = getattr(self, name)
                before = path.read_bytes()
                path.write_bytes(b'')
                with patch('release_bundle.os.link', wraps=os.link) as link:
                    with self.assertRaises(ValueError):
                        self.commit()
                link.assert_not_called()
                path.write_bytes(before)
        self.ready.write_bytes(b'x' * 32768)
        self.commit()
        self.assertEqual(self.record.read_bytes(), b'x' * 32768)

    def test_oversized_archive_is_rejected_without_reading_or_linking(self):
        # Sparse fixture: the reader must reject metadata before allocating/reading it.
        with self.packed.open('wb') as output:
            output.truncate(134217729)
        with patch('release_bundle.os.read', side_effect=AssertionError('oversized archive read')), \
                patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaises(ValueError):
                self.commit()
        link.assert_not_called()

    def test_source_replacement_between_lstat_and_open_is_rejected(self):
        replacement = self.root / 'replacement'
        replacement.write_bytes(self.payload)
        original_open = os.open
        replaced = False

        def open_swapped(path, *args, **kwargs):
            nonlocal replaced
            if isinstance(path, (str, os.PathLike)) and Path(path) == self.packed and not replaced:
                os.replace(replacement, self.packed)
                replaced = True
            return original_open(path, *args, **kwargs)

        with patch('release_bundle.os.open', side_effect=open_swapped), \
                patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(replaced)
        link.assert_not_called()

    def test_growth_during_read_uses_one_sentinel_byte_and_rejects(self):
        original_read = os.read
        inode = self.packed.stat().st_ino
        requests = []
        changed = False

        def read_growing(descriptor, size):
            nonlocal changed
            if os.fstat(descriptor).st_ino == inode:
                requests.append(size)
                if not changed:
                    with self.packed.open('ab') as output:
                        output.write(b'x' * 4096)
                    changed = True
            return original_read(descriptor, size)

        with patch('release_bundle.os.read', side_effect=read_growing), \
                patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaises(ValueError):
                self.commit()
        self.assertTrue(changed)
        self.assertEqual(requests, [len(self.payload) + 1])
        link.assert_not_called()

    def test_record_changed_during_final_archive_scan_is_rejected(self):
        original_open = os.open
        archive_reads = 0

        def open_interleaved(path, *args, **kwargs):
            nonlocal archive_reads
            if isinstance(path, (str, os.PathLike)) and Path(path) == self.archive:
                archive_reads += 1
                if archive_reads == 2:
                    before = self.record.stat()
                    self.record.write_bytes(b'x' * len(self.metadata))
                    os.utime(self.record, ns=(before.st_atime_ns, before.st_mtime_ns + 5_000_000_000))
            return original_open(path, *args, **kwargs)

        with patch('release_bundle.os.open', side_effect=open_interleaved):
            with self.assertRaises(ValueError):
                self.commit()
        self.assertEqual(archive_reads, 2, 'mutation must run after the record snapshot')
        self.assertTrue(self.record.exists(), 'failed validation must not delete shared paths')

    def test_final_revalidation_ablation_is_caught_by_the_same_file_regression(self):
        source = inspect.getsource(release_bundle.commit_release_inputs)
        marker = '    committed_record = check_record()'
        self.assertEqual(source.count(marker), 1)
        mutated = source.split(marker, 1)[0] + '    return\n'
        namespace = dict(vars(release_bundle))
        exec(compile(mutated, '<isolated-commit-validation-ablation>', 'exec'), namespace)
        suite = unittest.TestSuite([ReleaseCommitIntegrity(
            'test_record_mutation_after_link_cannot_change_the_comparison_baseline')])
        with patch.object(release_bundle, 'commit_release_inputs', namespace['commit_release_inputs']):
            result = unittest.TextTestRunner(stream=io.StringIO()).run(suite)
        self.assertEqual(result.testsRun, 1)
        self.assertEqual(result.errors, [], 'syntax/runtime errors are not a mutation kill')
        self.assertEqual(len(result.failures), 1)
        self.assertIn('ValueError not raised', result.failures[0][1])


if __name__ == '__main__':
    unittest.main()