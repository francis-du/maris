"""Keep native route implementations wired to APIs from their own platform."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NativeModuleContract(unittest.TestCase):
    def test_platform_modules_do_not_use_the_other_platforms_route_methods(self):
        pulse = (ROOT / 'src/audio/pulse/mod.rs').read_text(encoding='utf-8')
        self.assertNotIn('rebind_windows_output', pulse)
        self.assertIn('fn refresh_pulse', pulse)
        windows = (ROOT / 'src/audio/windows.rs').read_text(encoding='utf-8')
        self.assertNotIn('flexaudio::processes(', windows)
        self.assertIn('super::windows_route::applications()', windows)


if __name__ == '__main__':
    unittest.main()
