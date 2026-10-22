import io
import stat
import sys
from pathlib import Path
from unittest import TestCase, mock
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import package_smoke


LIMIT = 536_870_912


class _SizedEntries:
    """Expose a boundary count without allocating or extracting 10,000 paths."""

    def __init__(self, reported_len, entries):
        self.reported_len = reported_len
        self.entries = entries

    def __len__(self):
        return self.reported_len

    def __iter__(self):
        return iter(self.entries)


class _TarEntry:
    def __init__(self, size=0, name="maris-limit/payload", directory=False):
        self.name = name
        self.size = size
        self.mode = 0o755 if directory else 0o644
        self._directory = directory

    def isdir(self):
        return self._directory

    def isfile(self):
        return not self._directory


class _TarArchive:
    def __init__(self, size=None, entries=None, reported_len=None):
        if entries is None:
            entries = [_TarEntry(size=size)]
        self._entries = (
            _SizedEntries(reported_len, entries) if reported_len is not None else entries
        )

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def getmembers(self):
        return self._entries

    def extractfile(self, _entry):
        # Size-limit tests exercise trusted archive metadata without writing 512 MiB.
        return io.BytesIO(b"")


class _ZipEntry:
    def __init__(self, size=0, name="maris-limit/payload", directory=False):
        self.filename = name + ("/" if directory and not name.endswith("/") else "")
        self.file_size = size
        self.external_attr = ((stat.S_IFDIR if directory else stat.S_IFREG) | 0o644) << 16
        self._directory = directory

    def is_dir(self):
        return self._directory


class _ZipArchive:
    def __init__(self, size=None, entries=None, reported_len=None):
        if entries is None:
            entries = [_ZipEntry(size=size)]
        self._entries = (
            _SizedEntries(reported_len, entries) if reported_len is not None else entries
        )

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def infolist(self):
        return self._entries

    def testzip(self):
        return None

    def open(self, _entry):
        # Size-limit tests exercise trusted archive metadata without writing 512 MiB.
        return io.BytesIO(b"")


class ArchiveLimitBoundaries(TestCase):
    def test_tar_expanded_size_accepts_exact_limit_and_rejects_one_byte_more(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "exact"
            with mock.patch.object(package_smoke.tarfile, "open", return_value=_TarArchive(LIMIT)):
                package_smoke.extract(Path("unused.tar.gz"), destination, "linux", "maris-limit")
            self.assertTrue((destination / "maris-limit" / "payload").is_file())

            with mock.patch.object(
                package_smoke.tarfile, "open", return_value=_TarArchive(LIMIT + 1)
            ):
                with self.assertRaisesRegex(ValueError, "Unsafe archive entry"):
                    package_smoke.extract(
                        Path("unused.tar.gz"),
                        Path(temporary) / "too-large",
                        "linux",
                        "maris-limit",
                    )

    def test_zip_expanded_size_accepts_exact_limit_and_rejects_one_byte_more(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "exact"
            with mock.patch.object(package_smoke.zipfile, "ZipFile", return_value=_ZipArchive(LIMIT)):
                package_smoke.extract(Path("unused.zip"), destination, "windows", "maris-limit")
            self.assertTrue((destination / "maris-limit" / "payload").is_file())

            with mock.patch.object(
                package_smoke.zipfile, "ZipFile", return_value=_ZipArchive(LIMIT + 1)
            ):
                with self.assertRaisesRegex(ValueError, "Unsafe archive entry"):
                    package_smoke.extract(
                        Path("unused.zip"),
                        Path(temporary) / "too-large",
                        "windows",
                        "maris-limit",
                    )

    def test_tar_entry_count_accepts_exact_limit_and_rejects_one_more(self):
        valid = [_TarEntry(name="maris-limit/payload", directory=True)]
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(
                package_smoke.tarfile,
                "open",
                return_value=_TarArchive(entries=valid, reported_len=10_000),
            ):
                package_smoke.extract(
                    Path("unused.tar.gz"),
                    Path(temporary) / "exact",
                    "linux",
                    "maris-limit",
                )
            with mock.patch.object(
                package_smoke.tarfile,
                "open",
                return_value=_TarArchive(entries=valid, reported_len=10_001),
            ):
                with self.assertRaisesRegex(ValueError, "Archive file-count limit exceeded"):
                    package_smoke.extract(
                        Path("unused.tar.gz"),
                        Path(temporary) / "too-many",
                        "linux",
                        "maris-limit",
                    )

    def test_zip_entry_count_accepts_exact_limit_and_rejects_one_more(self):
        valid = [_ZipEntry(name="maris-limit/payload", directory=True)]
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(
                package_smoke.zipfile,
                "ZipFile",
                return_value=_ZipArchive(entries=valid, reported_len=10_000),
            ):
                package_smoke.extract(
                    Path("unused.zip"),
                    Path(temporary) / "exact",
                    "windows",
                    "maris-limit",
                )
            with mock.patch.object(
                package_smoke.zipfile,
                "ZipFile",
                return_value=_ZipArchive(entries=valid, reported_len=10_001),
            ):
                with self.assertRaisesRegex(ValueError, "Archive file-count limit exceeded"):
                    package_smoke.extract(
                        Path("unused.zip"),
                        Path(temporary) / "too-many",
                        "windows",
                        "maris-limit",
                    )
