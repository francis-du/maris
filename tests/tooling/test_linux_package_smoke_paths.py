import io
import sys
from pathlib import Path
import tarfile
import tempfile
from unittest import TestCase

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import package_smoke


class LinuxPackageSmokePaths(TestCase):
    def write_tar(self, root: Path, entries: list[tuple[str, int]]) -> Path:
        archive = root / "fixture.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for name, mode in entries:
                info = tarfile.TarInfo(name)
                info.mode = mode
                if name.endswith("/"):
                    info.type = tarfile.DIRTYPE
                    info.size = 0
                    output.addfile(info)
                else:
                    payload = b"x"
                    info.size = len(payload)
                    output.addfile(info, io.BytesIO(payload))
        return archive

    def assert_rejected(self, entries: list[tuple[str, int]]) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.write_tar(root, entries)
            with self.assertRaisesRegex(ValueError, "Unsafe archive entry|Archive file-count limit exceeded"):
                package_smoke.extract(archive, root / "out", "linux", "Maris")

    def test_valid_linux_tar_extracts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.write_tar(
                root,
                [
                    ("Maris/", 0o755),
                    ("Maris/bin/", 0o755),
                    ("Maris/bin/maris", 0o755),
                    ("Maris/resources/LICENSE", 0o644),
                ],
            )
            destination = root / "out"
            package_smoke.extract(archive, destination, "linux", "Maris")
            self.assertEqual((destination / "Maris/bin/maris").read_bytes(), b"x")

    def test_linux_smoke_rejects_paths_real_installer_rejects(self):
        for name in ["Maris/C:evil", "Maris/name with space", "__MACOSX/extra"]:
            with self.subTest(name=name):
                self.assert_rejected([("Maris/", 0o755), (name, 0o644)])

    def test_case_collisions_and_parent_spelling_collisions_are_rejected(self):
        self.assert_rejected(
            [("Maris/", 0o755), ("Maris/File", 0o644), ("Maris/file", 0o644)]
        )
        self.assert_rejected(
            [
                ("Maris/", 0o755),
                ("Maris/Foo/", 0o755),
                ("Maris/Foo/a", 0o644),
                ("Maris/foo/b", 0o644),
            ]
        )

    def test_privileged_mode_and_empty_archive_are_rejected(self):
        self.assert_rejected([("Maris/", 0o755), ("Maris/bin/maris", 0o4755)])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.write_tar(root, [])
            with self.assertRaisesRegex(ValueError, "Archive file-count limit exceeded"):
                package_smoke.extract(archive, root / "out", "linux", "Maris")
