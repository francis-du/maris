import sys
from pathlib import Path
from types import SimpleNamespace
from unittest import TestCase, mock
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import package_smoke


LIMIT = 536_870_912


class _TarEntry:
    def __init__(self, size):
        self.name = "maris-limit/payload"
        self.size = size
        self.mode = 0
    def isdir(self):
        return True
    def isfile(self):
        return False


class _TarArchive:
    def __init__(self, size):
        self._entry = _TarEntry(size)
    def __enter__(self):
        return self
    def __exit__(self, *_):
        return False
    def getmembers(self):
        return [self._entry]


class _ZipEntry:
    def __init__(self, size):
        self.filename = "maris-limit/payload/"
        self.file_size = size
        self.external_attr = 0
    def is_dir(self):
        return True


class _ZipArchive:
    def __init__(self, size):
        self._entry = _ZipEntry(size)
    def __enter__(self):
        return self
    def __exit__(self, *_):
        return False
    def infolist(self):
        return [self._entry]
    def testzip(self):
        return None


class ArchiveLimitBoundaries(TestCase):
    def test_tar_expanded_size_accepts_exact_limit_and_rejects_one_byte_more(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "exact"
            with mock.patch.object(package_smoke.tarfile, "open", return_value=_TarArchive(LIMIT)):
                package_smoke.extract(Path("unused.tar.gz"), destination, "linux", "maris-limit")
            self.assertTrue((destination / "maris-limit" / "payload").is_dir())

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
            self.assertTrue((destination / "maris-limit" / "payload").is_dir())

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
