"""Release-pair races using real hard links and isolated, non-executable bytes.

No download, user installation, native binary execution or public publication.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import inspect
import io
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import release_bundle
from release_bundle import commit_release_inputs


class ReleaseCommitRaces(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='maris-pair-race-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.packed = self.root / 'prepared.archive'
        self.ready = self.root / 'prepared.json'
        self.peer_packed = self.root / 'peer.archive'
        self.peer_ready = self.root / 'peer.json'
        self.archive = self.root / 'final.archive'
        self.record = self.root / 'final.release.json'
        self.payload = b'non-executable exact archive fixture\n'
        self.metadata = b'{"producer":"same-ci-run"}\n'
        for path in (self.packed, self.peer_packed):
            path.write_bytes(self.payload)
        for path in (self.ready, self.peer_ready):
            path.write_bytes(self.metadata)
        self.expected = hashlib.sha256(self.payload).hexdigest()

    def commit(self, peer=False):
        commit_release_inputs(
            self.peer_packed if peer else self.packed,
            self.peer_ready if peer else self.ready,
            self.archive, self.record, self.expected,
        )

    def test_late_io_failure_cannot_remove_a_peers_completed_pair(self):
        real_link = os.link
        injected = False
        winner_identity = None

        def interleave(source, destination):
            nonlocal injected, winner_identity
            if source == self.ready and destination == self.record:
                injected = True
                self.commit(peer=True)
                winner_identity = (self.archive.stat().st_ino, self.record.stat().st_ino)
                raise OSError('injected losing record-link failure')
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaisesRegex(OSError, 'losing record-link failure'):
                self.commit()
        self.assertTrue(injected, 'the scheduled peer commit must actually run')
        self.assertTrue(self.archive.is_file(), 'loser deleted the successful peers archive')
        self.assertEqual(self.archive.read_bytes(), self.payload)
        self.assertEqual(self.record.read_bytes(), self.metadata)
        self.assertEqual((self.archive.stat().st_ino, self.record.stat().st_ino), winner_identity)

    def test_conflicting_provenance_loser_preserves_the_winning_pair(self):
        self.peer_ready.write_bytes(b'{"producer":"different-ci-run"}\n')
        real_link = os.link
        injected = False

        def interleave(source, destination):
            nonlocal injected
            if source == self.ready and destination == self.record:
                injected = True
                self.commit(peer=True)
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interleave):
            with self.assertRaisesRegex(ValueError, 'record differs'):
                self.commit()
        self.assertTrue(injected)
        self.assertTrue(self.archive.is_file(), 'conflicting loser removed the winners payload')
        self.assertEqual(self.archive.read_bytes(), self.payload)
        self.assertEqual(self.record.read_bytes(), self.peer_ready.read_bytes())
        self.commit(peer=True)

    def test_record_link_failure_preserves_an_exact_retryable_orphan(self):
        real_link = os.link

        def fail_record(source, destination):
            if destination == self.record:
                raise OSError('injected record storage failure')
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=fail_record):
            with self.assertRaisesRegex(OSError, 'record storage failure'):
                self.commit()
        self.assertFalse(self.record.exists(), 'failure must not invent a successful record')
        self.assertTrue(self.archive.is_file(), 'published archive was destructively rolled back')
        inode = self.archive.stat().st_ino
        self.commit()
        self.assertEqual(self.archive.stat().st_ino, inode)
        self.assertEqual(self.archive.read_bytes(), self.payload)
        self.assertEqual(self.record.read_bytes(), self.metadata)

    def test_existing_conflicting_record_is_rejected_before_any_archive_link(self):
        self.record.write_bytes(b'previous provenance must remain unchanged')
        with patch('release_bundle.os.link', wraps=os.link) as link:
            with self.assertRaisesRegex(ValueError, 'record differs'):
                self.commit()
        link.assert_not_called()
        self.assertFalse(self.archive.exists())
        self.assertEqual(self.record.read_bytes(), b'previous provenance must remain unchanged')

    def test_abrupt_stop_after_archive_link_is_recoverable_without_replacement(self):
        real_link = os.link

        def interrupt(source, destination):
            if destination == self.record:
                raise KeyboardInterrupt('injected process interruption')
            return real_link(source, destination)

        with patch('release_bundle.os.link', side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.commit()
        self.assertFalse(self.record.exists())
        inode = self.archive.stat().st_ino
        self.commit(peer=True)
        self.assertEqual(self.archive.stat().st_ino, inode)
        self.assertEqual(self.record.read_bytes(), self.metadata)

    def test_mismatching_retry_preserves_orphan_and_never_creates_a_record(self):
        os.link(self.packed, self.archive)
        original_inode = self.archive.stat().st_ino
        self.peer_packed.write_bytes(b'different archive must not replace the orphan')
        different_hash = hashlib.sha256(self.peer_packed.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, 'archive differs'):
            commit_release_inputs(self.peer_packed, self.peer_ready, self.archive,
                                  self.record, different_hash)
        self.assertFalse(self.record.exists())
        self.assertEqual(self.archive.stat().st_ino, original_inode)
        self.assertEqual(self.archive.read_bytes(), self.payload)

    def test_independent_concurrent_preparers_converge_without_overwriting(self):
        for mixed_provenance in (False, True):
            with self.subTest(mixed_provenance=mixed_provenance):
                destination = self.root / str(mixed_provenance)
                destination.mkdir()
                archive = destination / 'final.archive'
                record = destination / 'final.release.json'
                inputs = []
                for index in range(8):
                    packed = destination / f'prepared-{index}.archive'
                    ready = destination / f'prepared-{index}.json'
                    packed.write_bytes(self.payload)
                    ready.write_bytes(f'ci-run-{index if mixed_provenance else 0}\n'.encode())
                    inputs.append((packed, ready))
                barrier = threading.Barrier(len(inputs))

                def publish(paths):
                    packed, ready = paths
                    barrier.wait(timeout=10)
                    try:
                        commit_release_inputs(packed, ready, archive, record, self.expected)
                    except ValueError as error:
                        return False, ready.read_bytes(), str(error)
                    return True, ready.read_bytes(), ''

                with ThreadPoolExecutor(max_workers=len(inputs)) as pool:
                    results = list(pool.map(publish, inputs))
                winners = [data for passed, data, _ in results if passed]
                self.assertEqual(len(winners), 1 if mixed_provenance else 8)
                self.assertEqual(archive.read_bytes(), self.payload)
                self.assertTrue(all(data == record.read_bytes() for data in winners))
                self.assertTrue(all('record differs' in error for passed, _, error in results if not passed))

    def test_orphan_fails_six_target_collection_until_exact_record_recovery(self):
        from test_release_kits import make_records
        collection = self.root / 'collection'
        collection.mkdir()
        record = make_records(collection)[0]
        prepared_record = self.root / 'retained-record.json'
        prepared_record.write_bytes(record.read_bytes())
        import json
        metadata = json.loads(record.read_bytes())
        archive = record.parent / metadata['archive']
        prepared_archive = self.root / 'retained-archive'
        prepared_archive.write_bytes(archive.read_bytes())
        inode = archive.stat().st_ino
        record.unlink()  # The sixth target has an archive, but no committed descriptor.
        manifest = self.root / 'manifest.tsv'
        with self.assertRaisesRegex(ValueError, 'six distinct'):
            release_bundle.combine(collection, manifest)
        self.assertFalse(manifest.exists(), 'an unpaired target passed the collector')
        commit_release_inputs(prepared_archive, prepared_record, archive, record, metadata['sha256'])
        self.assertEqual(archive.stat().st_ino, inode)
        self.assertEqual(release_bundle.combine(collection, manifest), 'candidate')
        self.assertIn('channel\tcandidate', manifest.read_text())

    def test_destructive_rollback_ablation_is_caught_by_the_peer_regression(self):
        # Change only an isolated function copy; never rewrite the shared source.
        source = inspect.getsource(release_bundle.commit_release_inputs)
        marker = '    # Never unlink the shared archive on failure:'
        self.assertEqual(source.count(marker), 1)
        mutated = source.replace(marker, '    except OSError:\n'
                                 '        archive.unlink(missing_ok=True)\n'
                                 '        raise\n' + marker, 1)
        namespace = dict(vars(release_bundle))
        exec(compile(mutated, '<isolated-release-rollback-ablation>', 'exec'), namespace)
        suite = unittest.TestSuite([
            ReleaseCommitRaces('test_late_io_failure_cannot_remove_a_peers_completed_pair')
        ])
        with patch.object(sys.modules[__name__], 'commit_release_inputs', namespace['commit_release_inputs']):
            result = unittest.TextTestRunner(stream=io.StringIO()).run(suite)
        self.assertEqual(result.testsRun, 1)
        self.assertEqual(result.errors, [], 'a compile/runtime error is not an ablation failure')
        self.assertEqual(len(result.failures), 1)
        self.assertIn('loser deleted the successful peers archive', result.failures[0][1])

    def test_normal_and_identical_retries_preserve_both_file_identities(self):
        self.commit()
        before = (self.archive.stat().st_ino, self.record.stat().st_ino)
        self.commit(peer=True)
        self.assertEqual((self.archive.stat().st_ino, self.record.stat().st_ino), before)
        self.assertEqual(self.archive.read_bytes(), self.payload)
        self.assertEqual(self.record.read_bytes(), self.metadata)


if __name__ == '__main__':
    unittest.main()