"""Keep native route implementations wired to APIs from their own platform."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NativeModuleContract(unittest.TestCase):
    def test_macos_architecture_checks_put_the_input_before_verify_arch(self):
        # LLVM consumes every trailing argument as an architecture name. The
        # installer must inspect the real binary rather than pass its path there.
        installer = (ROOT / 'scripts/install_macos.sh').read_text(encoding='utf-8')
        self.assertIn('lipo "$app/Contents/MacOS/maris" -verify_arch "$arch"', installer)
        package = (ROOT / 'scripts/package.py').read_text(encoding='utf-8')
        gate = (ROOT / 'scripts/release_gate.py').read_text(encoding='utf-8')
        self.assertIn('["/usr/bin/lipo", str(executable), "-verify_arch", arch]', package)
        self.assertIn('["/usr/bin/lipo", str(bundle / "Contents/MacOS/maris"), "-verify_arch", arch]', gate)
        for text in (installer, package, gate):
            self.assertNotIn('lipo -verify_arch', text)
            self.assertNotIn('["/usr/bin/lipo", "-verify_arch"', text)

    def test_platform_modules_do_not_use_the_other_platforms_route_methods(self):
        pulse = (ROOT / 'src/audio/pulse/mod.rs').read_text(encoding='utf-8')
        self.assertNotIn('rebind_windows_output', pulse)
        self.assertIn('fn refresh_pulse', pulse)
        windows = (ROOT / 'src/audio/windows.rs').read_text(encoding='utf-8')
        self.assertNotIn('flexaudio::processes(', windows)
        self.assertIn('super::windows_route::applications()', windows)


if __name__ == '__main__':
    unittest.main()
