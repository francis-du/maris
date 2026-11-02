"""Refresh orchestration regressions using synthetic buffers and mocked probes.

These fixtures exercise source binding and failure preservation only. No Cargo,
Ratatui process, AppKit capture, audio device or real documentation asset is used.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[2] / 'scripts/docs_assets.py'


class IllustrationRefresh(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        script = self.root / 'scripts/docs_assets.py'
        script.parent.mkdir(parents=True)
        script.write_bytes(SCRIPT.read_bytes())
        spec = importlib.util.spec_from_file_location('isolated_docs_assets_refresh', script)
        self.docs = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.docs)
        self.directory = self.root / 'docs/assets'
        self.directory.mkdir(parents=True)
        (self.root / '.maris-review').mkdir()
        for name in self.docs.SOURCES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('original synthetic source\n', encoding='utf-8')
        self.source_hashes = {
            name: self.docs.digest(self.root / name) for name in self.docs.SOURCES
        }
        for view in self.docs.VIEWS:
            for language in self.docs.LANGUAGES:
                name = f'{view}-{language}.svg'
                (self.directory / name).write_text(
                    f'<svg xmlns="http://www.w3.org/2000/svg"><text>Previous synthetic {name}</text></svg>\n',
                    encoding='utf-8',
                )
        (self.directory / 'illustrations.json').write_text(
            '{"fixture": "previous synthetic manifest bytes"}\n', encoding='utf-8'
        )
        self.previous = self.asset_bytes()
        cells = [
            {'x': x, 'y': y, 'display_width': 1, 'text': ' ',
             'fg': [255, 255, 255], 'bg': [0, 0, 0], 'bold': False}
            for y in range(40) for x in range(140)
        ]
        for index, character in enumerate('SYNTHETIC TEST FIXTURE'):
            cells[index]['text'] = character
        # The schema label is required to exercise export/validate; the visible
        # text and this test's scope make the buffer's synthetic origin clear.
        self.buffer = json.dumps({
            'label': 'Offline Ratatui render / no hardware session',
            'width': 140, 'height': 40, 'cells': cells,
        })
        self.commands = []
        self.output = io.StringIO()

    def asset_bytes(self):
        return {path.name: path.read_bytes() for path in self.directory.iterdir()}

    def successful_probe(self, command, **kwargs):
        self.commands.append(command)
        settings = '--settings' in command
        for view, prefix in self.docs.VIEWS.items():
            if (view == 'settings') != settings:
                continue
            for language in self.docs.LANGUAGES:
                suffix = '' if language == 'en' else '-' + language
                target = self.root / '.maris-review' / (prefix + suffix + '.json')
                target.write_text(self.buffer, encoding='utf-8')
        return subprocess.CompletedProcess(
            command, 0, stdout='Synthetic probe fixture completed.\n', stderr=''
        )

    def refresh(self, probe):
        with mock.patch.object(sys, 'argv', ['docs_assets.py', '--refresh']), \
                mock.patch.object(self.docs.subprocess, 'run', side_effect=probe), \
                contextlib.redirect_stdout(self.output):
            self.docs.main()

    def test_unchanged_sources_refresh_complete_fixture_gallery(self):
        self.refresh(self.successful_probe)
        manifest = self.docs.validate(self.directory, self.root)
        self.assertEqual(len(self.commands), 2)
        self.assertEqual(len(manifest['images']), 18)
        self.assertEqual(manifest['source_sha256'], self.source_hashes)
        self.assertFalse(manifest['hardware_validation'])
        self.assertNotEqual(self.asset_bytes(), self.previous)
        self.assertIn('Exported 18', self.output.getvalue())

    def test_change_between_probes_is_rejected_before_published_assets_are_written(self):
        def change_after_first_probe(command, **kwargs):
            result = self.successful_probe(command, **kwargs)
            if len(self.commands) == 1:
                (self.root / self.docs.SOURCES[0]).write_text(
                    'changed after the first synthetic render\n', encoding='utf-8'
                )
            return result

        with self.assertRaisesRegex(ValueError, 'sources changed during'):
            self.refresh(change_after_first_probe)
        self.assertEqual(self.asset_bytes(), self.previous)
        self.assertNotIn('Exported', self.output.getvalue())

    def assert_probe_error_preserves_assets(self, failure):
        for failed_stage in (1, 2):
            with self.subTest(failed_stage=failed_stage):
                self.commands.clear()
                self.output = io.StringIO()

                def fail(command, **kwargs):
                    if len(self.commands) + 1 == failed_stage:
                        raise failure(command)
                    return self.successful_probe(command, **kwargs)

                with self.assertRaises(failure([]).__class__):
                    self.refresh(fail)
                self.assertEqual(self.asset_bytes(), self.previous)
                self.assertNotIn('Exported', self.output.getvalue())

    def test_failed_probe_cannot_publish_or_report_success(self):
        self.assert_probe_error_preserves_assets(
            lambda command: subprocess.CalledProcessError(
                17, command, output='Synthetic partial output', stderr='Synthetic failure'
            )
        )

    def test_timed_out_probe_cannot_publish_or_report_success(self):
        self.assert_probe_error_preserves_assets(
            lambda command: subprocess.TimeoutExpired(
                command, 180, output='Synthetic partial output', stderr='Synthetic timeout'
            )
        )


if __name__ == '__main__':
    unittest.main()
