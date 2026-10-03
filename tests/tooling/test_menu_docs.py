"""Negative publication controls for synthetic, labeled native-capture receipts.

These small PNG fixtures test the documentation boundary, not AppKit rendering.
Actual AppKit screenshots remain the explicit production menu probe's evidence.
"""
from __future__ import annotations
import base64
import copy
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
import capture_menu_docs as docs


def synthetic_png() -> bytes:
    def chunk(name, content):
        value = name + content
        return struct.pack('>I', len(content)) + value + struct.pack('>I', zlib.crc32(value))
    row = b'\0' + b'\x00\x00\x00\xff' * 32
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 24, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(row * 24)) + chunk(b'IEND', b''))


class NativeMenuDocuments(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.directory = self.root / 'docs/assets'
        self.directory.mkdir(parents=True)
        for name in docs.SOURCES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('synthetic test source\n', encoding='utf-8')
        self.report = {'audio_started': False, 'hardware_validated': False,
                       'source': 'isolated offline fixture', 'native_menu_construction': True, 'locales': []}
        png = synthetic_png()
        for language in docs.LOCALES:
            captures = []
            for state, appearance in [('live', 'light')] + [(state, 'dark') for state in docs.STATES]:
                item = {'language': language, 'state': state, 'appearance': appearance,
                        'image': {'template': True, 'logical_width': 18, 'logical_height': 18,
                                  'representations': [{'width': 64, 'height': 64}],
                                  'accessibility_label': 'Maris', 'accessibility_value': 'Synthetic state'}}
                for kind in ('menu', 'bar', 'header'):
                    name = f'.maris-review/native-menu/{language}-{state}-{appearance}-{kind}.png'
                    path = self.root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(png)
                    item[kind + '_path'] = name
                    item[kind] = {'whole_desktop_captured': False, 'appearance': appearance,
                                  'renderer': 'AppKit cacheDisplayInRect:toBitmapImageRep:',
                                  'pixel_width': 32, 'pixel_height': 24, 'png_bytes': len(png),
                                  'logical_width': 32, 'logical_height': 24}
                captures.append(item)
            self.report['locales'].append({'language': language, 'native_states': {'captures': captures}})
        self.native = docs.captures(self.root, self.report)
        self.manifest = {'scope': 'actual_offline_appkit', 'hardware_validation': False,
                         'audio_started': False, 'whole_desktop_captured': False,
                         'source_sha256': docs.source_hashes(self.root), 'images': {}}
        for language, entries in self.native.items():
            document, captures = docs.illustration(self.root, language, entries)
            name = f'menu-bar-{language}.svg'
            data = document.encode('utf-8')
            (self.directory / name).write_bytes(data)
            self.manifest['images'][name] = {'sha256': docs.digest(data), 'captures': captures}
        self.save_manifest()

    def save_manifest(self):
        (self.directory / docs.MANIFEST).write_text(json.dumps(self.manifest), encoding='utf-8')

    def change_svg(self, replace):
        path = self.directory / 'menu-bar-en.svg'
        data = replace(path.read_text(encoding='utf-8')).encode('utf-8')
        path.write_bytes(data)
        self.manifest['images'][path.name]['sha256'] = docs.digest(data)
        self.save_manifest()

    def test_source_bound_embedded_capture_roundtrip(self):
        self.assertEqual(len(docs.validate(self.directory, self.root)['images']), 2)

    def test_source_change_requires_fresh_production_probe(self):
        (self.root / docs.SOURCES[0]).write_text('changed\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'stale'):
            docs.validate(self.directory, self.root)

    def test_rehashed_svg_cannot_load_external_image(self):
        encoded = base64.b64encode(synthetic_png()).decode('ascii')
        self.change_svg(lambda text: text.replace('data:image/png;base64,' + encoded, 'https://example.invalid/image.png', 1))
        with self.assertRaisesRegex(ValueError, 'External'):
            docs.validate(self.directory, self.root)

    def test_rehashed_svg_cannot_add_active_content(self):
        self.change_svg(lambda text: text.replace('</svg>', '<script>alert(1)</script></svg>'))
        with self.assertRaisesRegex(ValueError, 'Active'):
            docs.validate(self.directory, self.root)

    def test_desktop_capture_cannot_be_published_as_view_capture(self):
        item = copy.deepcopy(self.native['en'][('live', 'light')])
        item['menu']['whole_desktop_captured'] = True
        with self.assertRaisesRegex(ValueError, 'provenance'):
            docs.image_source(self.root, item, 'menu')

    def test_wrapped_review_title_is_not_a_menu_bar(self):
        item = copy.deepcopy(self.native['en'][('live', 'light')])
        item['bar']['logical_height'] = 176
        with self.assertRaisesRegex(ValueError, 'single native'):
            docs.image_source(self.root, item, 'bar')

    def test_missing_required_state_cannot_pass_as_complete_gallery(self):
        self.report['locales'][0]['native_states']['captures'] = [
            item for item in self.report['locales'][0]['native_states']['captures'] if item['state'] != 'stale']
        with self.assertRaisesRegex(ValueError, 'Missing native'):
            docs.captures(self.root, self.report)

    def test_linked_capture_cannot_escape_isolated_probe(self):
        item = self.native['en'][('live', 'light')]
        path = self.root / item['menu_path']
        path.unlink()
        path.symlink_to(self.root / 'Cargo.toml')
        with self.assertRaisesRegex(ValueError, 'Linked'):
            docs.image_source(self.root, item, 'menu')


if __name__ == '__main__':
    unittest.main()
