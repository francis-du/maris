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
            '"coreaudio_fail_open_reason"',
        ):
            with self.subTest(field=field):
                self.assertIn(field, source)
        self.assertIn("capture_callback_calls.load(Ordering::Acquire)", source)
        self.assertIn("callback_calls.load(Ordering::Acquire)", source)
        self.assertIn("output_quarantined.load(Ordering::Acquire)", source)
        self.assertIn("errors.load(Ordering::Acquire)", source)

    def test_teardown_snapshots_diagnostics_before_native_resources_are_released(self):
        source = (ROOT / "src/audio/mod.rs").read_text(encoding="utf-8")
        drop = source.split("impl Drop for Session {", 1)[1].split("fn new_session_id", 1)[0]
        snapshot = "session_control::final_runtime_status(self.status())"
        self.assertLess(drop.index(snapshot), drop.index("self.tap.take()"))
        self.assertIn('self.store.write_json("runtime.json", &final_status)', drop)
