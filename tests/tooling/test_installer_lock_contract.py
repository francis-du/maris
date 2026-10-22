from pathlib import Path
from unittest import TestCase

ROOT = Path(__file__).resolve().parents[2]


class InstallerLockContract(TestCase):
    def test_unix_installers_bind_lock_cleanup_to_owner_marker(self):
        for relative in [
            "scripts/install_linux.sh",
            "scripts/install_cli.sh",
            "scripts/install_macos.sh",
        ]:
            with self.subTest(path=relative):
                text = (ROOT / relative).read_text(encoding="utf-8")
                self.assertIn('LOCK_OWNER="$$:$PPID"', text)
                self.assertIn('"$LOCK/.owner"', text)
                self.assertIn('"$(cat "$LOCK/.owner" 2>/dev/null || true)" = "$LOCK_OWNER"', text)

    def test_windows_lock_is_deleted_with_the_owned_file_handle(self):
        text = (ROOT / "install.ps1").read_text(encoding="utf-8")
        self.assertIn("[IO.FileOptions]::DeleteOnClose", text)
        self.assertIn("$lockHandle.Dispose()", text)
        self.assertNotIn("[IO.File]::Delete($lockPath)", text)
