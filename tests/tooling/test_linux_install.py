"""Native Linux filesystem/process fixtures. They are not Maris audio or device validation."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LINUX_USER = sys.platform == 'linux' and os.geteuid() != 0
PROGRAM = r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    const char *marker = getenv("MARIS_INSTALL_SENTINEL");
    if (marker) { FILE *file = fopen(marker, "a"); if (file) { fputs("executed\n", file); fclose(file); } }
    if (argc > 1 && strcmp(argv[1], "hold") == 0) { char c; while (read(0, &c, 1) > 0) {} }
    return 0;
}
'''


@unittest.skipUnless(LINUX_USER, 'Native Linux non-root installation fixtures')
class LinuxInstall(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for name in ['bash', 'cc', 'fuser']:
            if shutil.which(name) is None:
                raise RuntimeError('Native Linux fixture dependency missing: ' + name)
        cls.compilation = tempfile.TemporaryDirectory(prefix='maris-elf-')
        cls.binary = Path(cls.compilation.name) / 'fixture'
        subprocess.run(['cc', '-x', 'c', '-o', str(cls.binary), '-'], input=PROGRAM.encode(), check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.compilation.cleanup()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='maris-install-')
        self.root = Path(self.temporary.name).resolve()
        self.source = self.root / 'payload with spaces' / 'Maris'
        (self.source / 'bin').mkdir(parents=True)
        (self.source / 'resources').mkdir()
        shutil.copy2(self.binary, self.source / 'bin/maris')
        (self.source / 'resources/notice.txt').write_text('fixture notice')
        arch = 'x86_64' if os.uname().machine == 'x86_64' else 'arm64'
        (self.source / '.maris-package').write_text(f'maris-package-v1\nlinux\n{arch}\n0.1.0\n')
        self.prefix = self.root / 'prefix with spaces'
        self.sentinel = self.root / 'executed.txt'

    def tearDown(self):
        self.temporary.cleanup()

    def call(self, *options, env=None, source=None, prefix=None):
        return subprocess.run(['bash', str(ROOT / 'scripts/install_linux.sh'), '--from', str(source or self.source),
                               '--prefix', str(prefix or self.prefix), *options], capture_output=True, text=True,
                              env=dict(os.environ, MARIS_INSTALL_SENTINEL=str(self.sentinel), **(env or {})), timeout=20)

    def install(self):
        result = self.call('--allow-unsigned', '--yes')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_dry_run_and_missing_confirmation_are_read_only(self):
        result = self.call('--allow-unsigned', '--dry-run')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.prefix.exists())
        self.assertNotEqual(self.call('--allow-unsigned').returncode, 0)
        self.assertFalse(self.prefix.exists())
        self.assertFalse(self.sentinel.exists())

    def test_explicit_binary_hash_is_verified_before_writes(self):
        self.assertNotEqual(self.call('--yes').returncode, 0)
        self.assertNotEqual(self.call('--sha256', '0' * 64, '--yes').returncode, 0)
        self.assertFalse(self.prefix.exists())
        digest = hashlib.sha256((self.source / 'bin/maris').read_bytes()).hexdigest()
        result = self.call('--sha256', digest, '--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.sentinel.exists())

    def test_install_and_upgrade_preserve_notices_backup_and_unrelated_files(self):
        self.prefix.mkdir()
        (self.prefix / 'keep.txt').write_text('unrelated')
        self.install()
        installed = self.prefix / 'lib/maris'
        self.assertEqual(os.readlink(self.prefix / 'bin/maris'), '../lib/maris/bin/maris')
        self.assertEqual((installed / 'resources/notice.txt').read_text(), 'fixture notice')
        (installed / 'old.txt').write_text('previous')
        self.install()
        backups = list(self.prefix.glob('.maris-backup.*'))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / 'Maris/old.txt').read_text(), 'previous')
        self.assertFalse((installed / 'old.txt').exists())
        self.assertEqual((self.prefix / 'keep.txt').read_text(), 'unrelated')
        self.assertFalse((self.prefix / '.maris-install.lock').exists())
        self.assertFalse(self.sentinel.exists())

    def test_foreign_command_and_unowned_payload_are_not_replaced(self):
        (self.prefix / 'bin').mkdir(parents=True)
        command = self.prefix / 'bin/maris'
        command.write_text('unrelated')
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        self.assertEqual(command.read_text(), 'unrelated')
        command.unlink()
        (self.prefix / 'lib/maris').mkdir(parents=True)
        (self.prefix / 'lib/maris/keep.txt').write_text('unrelated')
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        self.assertTrue((self.prefix / 'lib/maris/keep.txt').exists())

    def test_symlink_components_and_source_links_are_rejected(self):
        alias = self.root / 'alias'
        alias.symlink_to(self.root, target_is_directory=True)
        self.assertNotEqual(self.call('--allow-unsigned', '--yes', prefix=alias / 'bad').returncode, 0)
        self.assertFalse((self.root / 'bad').exists())
        (self.source / 'resources/link').symlink_to('../.maris-package')
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_wrong_platform_and_corrupted_elf_are_rejected(self):
        marker = self.source / '.maris-package'
        original = marker.read_text()
        marker.write_text(original.replace('linux', 'windows'))
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        marker.write_text(original)
        binary = self.source / 'bin/maris'
        data = bytearray(binary.read_bytes()); data[18] = 0
        binary.write_bytes(data)
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_install_lock_is_preserved_on_rejection(self):
        (self.prefix / '.maris-install.lock').mkdir(parents=True)
        self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
        self.assertTrue((self.prefix / '.maris-install.lock').is_dir())
        self.assertFalse((self.prefix / 'lib/maris').exists())

    def test_failed_promotion_restores_previous_installation(self):
        self.install()
        (self.prefix / 'lib/maris/old.txt').write_text('previous')
        tools = self.root / 'fault-tools'; tools.mkdir()
        move = tools / 'mv'
        move.write_text('#!/bin/bash\ncase "$2" in */.maris-stage.*/Maris) exit 23;; esac\nexec /bin/mv "$@"\n')
        move.chmod(0o755)
        result = self.call('--allow-unsigned', '--yes', env={'PATH': str(tools) + ':/usr/bin:/bin:/usr/sbin:/sbin'})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.prefix / 'lib/maris/old.txt').read_text(), 'previous')
        self.assertTrue((self.prefix / 'bin/maris').exists())
        self.assertFalse((self.prefix / '.maris-install.lock').exists())

    def test_in_use_binary_is_not_replaced_or_terminated(self):
        self.install()
        child = subprocess.Popen([str(self.prefix / 'bin/maris'), 'hold'], stdin=subprocess.PIPE)
        try:
            self.assertNotEqual(self.call('--allow-unsigned', '--yes').returncode, 0)
            self.assertIsNone(child.poll())
        finally:
            child.stdin.close()
            child.wait(timeout=5)

    def test_same_source_and_destination_are_rejected(self):
        self.install()
        self.assertNotEqual(self.call('--allow-unsigned', '--yes', source=self.prefix / 'lib/maris').returncode, 0)
        self.assertTrue((self.prefix / 'lib/maris/bin/maris').is_file())


if __name__ == '__main__':
    unittest.main()
