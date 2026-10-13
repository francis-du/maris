from pathlib import Path
from unittest import TestCase

ROOT = Path(__file__).resolve().parents[2]


class InstallerRecoveryContract(TestCase):
    def test_bundled_unix_cli_installer_reports_blocked_rollback_backup(self):
        text = (ROOT / "scripts/install_cli.sh").read_text(encoding="utf-8")
        self.assertIn('if [ ! -e "$DEST" ]; then', text)
        self.assertIn("else\n                printf 'Recovery required: %s/Maris", text)

    def test_macos_gui_installer_reports_blocked_rollback_backup(self):
        text = (ROOT / "scripts/install_macos.sh").read_text(encoding="utf-8")
        self.assertIn('if [ ! -e "$DEST" ]; then', text)
        self.assertIn("else\n            printf 'Recovery required: previous bundle remains at %s", text)

    def test_windows_installer_reports_blocked_rollback_backup(self):
        text = (ROOT / "install.ps1").read_text(encoding="utf-8")
        self.assertIn("if (-not (Test-Path -LiteralPath $destination))", text)
        self.assertIn('else {\n            [Console]::Error.WriteLine("Recovery required: $backup\\Maris")', text)
