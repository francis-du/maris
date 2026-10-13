import sys
from pathlib import Path
import tempfile
from unittest import TestCase
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import package_smoke


class WindowsPackageSmokePaths(TestCase):
    def write_zip(self, root: Path, names: list[str]) -> Path:
        archive = root / "fixture.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for name in names:
                output.writestr(name, b"" if name.endswith("/") else b"x")
        return archive

    def assert_rejected(self, names: list[str]) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.write_zip(root, names)
            with self.assertRaisesRegex(ValueError, "Unsafe archive entry"):
                package_smoke.extract(archive, root / "out", "windows", "Maris")

    def test_valid_windows_zip_extracts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.write_zip(
                root,
                ["Maris/", "Maris/bin/", "Maris/bin/maris.exe", "Maris/resources/LICENSE"],
            )
            destination = root / "out"
            package_smoke.extract(archive, destination, "windows", "Maris")
            self.assertEqual((destination / "Maris/bin/maris.exe").read_bytes(), b"x")

    def test_drive_colon_reserved_names_and_trailing_dot_are_rejected(self):
        for name in ["Maris/C:evil", "Maris/CON", "Maris/name.", "Maris/name "]:
            with self.subTest(name=name):
                self.assert_rejected(["Maris/", name])

    def test_case_colliding_entries_are_rejected_like_the_installer(self):
        self.assert_rejected(["Maris/", "Maris/File", "Maris/file"])

    def test_directory_file_type_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "fixture.zip"
            info = zipfile.ZipInfo("Maris/not-a-directory/")
            info.external_attr = (0o100644 << 16)
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr(info, b"")
            with self.assertRaisesRegex(ValueError, "Unsafe archive entry"):
                package_smoke.extract(archive, root / "out", "windows", "Maris")
