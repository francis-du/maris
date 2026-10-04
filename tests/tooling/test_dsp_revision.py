from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DspRevisionContract(unittest.TestCase):
    def test_documented_fidelity_revision_matches_runtime_constant(self):
        source = (ROOT / 'src/dsp/mod.rs').read_text(encoding='utf-8')
        match = re.search(r'pub const DSP_REVISION: &str = "([^"]+)";', source)
        self.assertIsNotNone(match)
        revision = match.group(1)
        self.assertRegex(revision, r'^music-fidelity-[1-9][0-9]*$')
        for path in (
            ROOT / 'docs/reference/audio-quality.md',
            ROOT / 'docs/reference/commands.md',
        ):
            with self.subTest(path=str(path.relative_to(ROOT))):
                text = path.read_text(encoding='utf-8')
                self.assertIn(f'`{revision}`', text)
                declared = set(re.findall(r'`music-fidelity-[1-9][0-9]*`', text))
                self.assertEqual(declared, {f'`{revision}`'})


if __name__ == '__main__':
    unittest.main()
