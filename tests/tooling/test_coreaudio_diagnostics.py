from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CoreAudioDiagnosticsContract(unittest.TestCase):
    def test_macos_status_keeps_handoff_liveness_fields(self):
        source = "\n".join(
            (ROOT / path).read_text(encoding="utf-8")
            for path in ("src/audio/mod.rs", "src/audio/system.rs")
        )
        for field in (
            '"coreaudio_capture_callback_calls"',
            '"output_callback_calls"',
            '"output_quarantined"',
            '"audio_callback_errors"',
        ):
            with self.subTest(field=field):
                self.assertIn(field, source)
        self.assertIn("capture_callback_calls.load(Ordering::Acquire)", source)
        self.assertIn("callback_calls.load(Ordering::Acquire)", source)
        self.assertIn("output_quarantined.load(Ordering::Acquire)", source)
        self.assertIn("errors.load(Ordering::Acquire)", source)
