#!/usr/bin/env python3
"""Export reviewed SVG illustrations from actual offline Ratatui buffers; no invented UI.

Refresh is explicit and runs the existing offline probe. Normal site builds neither
execute Rust nor read user runtime state, fonts, network resources or audio devices.
"""
from __future__ import annotations
import argparse
from html import escape
import hashlib
import json
import os
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
LANGUAGES = ('en', 'zh-CN', 'zh-TW', 'ja', 'de', 'es')
VIEWS = {'studio': 'studio-140x40-signal', 'settings': 'inspector-eq-140x40-draft', 'presets': 'studio-140x40-picker'}
SOURCES = ('src/ui/tui/view.rs', 'src/ui/tui/studio/mod.rs', 'src/ui/tui/studio/analyzer.rs', 'src/ui/tui/studio/appearance.rs',
           'src/ui/tui/studio/controls.rs', 'src/ui/tui/monitor.rs', 'src/ui/tui/settings/view.rs',
           'src/ui/tui/presets.rs', 'src/ui/tui/output_picker.rs', 'src/ui/tui/preset_picker.rs', 'src/ui/tui/inspector.rs',
           'src/presets/mod.rs', 'src/presets/scenes.rs', 'src/i18n/mod.rs', 'src/i18n/console.rs',
           'src/i18n/surface.rs', 'src/i18n/messages.rs', 'tests/support/ui_probe.rs')


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export(snapshot: dict) -> str:
    if snapshot.get('label') != 'Offline Ratatui render / no hardware session':
        raise ValueError('Illustrations require a labeled offline probe, not user telemetry')
    width, height = snapshot['width'], snapshot['height']
    if (width, height) != (140, 40) or len(snapshot['cells']) != width * height:
        raise ValueError('Unexpected reviewed buffer dimensions')
    out = ['<!-- @generated from actual offline Ratatui cells; use the explicit docs_assets.py refresh command -->',
           '<svg xmlns="http://www.w3.org/2000/svg" width="1296" height="876" viewBox="0 0 1296 876" role="img" aria-labelledby="title desc">',
           '<title id="title">Maris actual offline UI render</title>',
           '<desc id="desc">Generated test audio analyzed by Maris and rendered by Ratatui. Not a hardware session or sound-quality validation.</desc>',
           '<rect width="1296" height="876" fill="#07090d"/>',
           '<g font-family="Menlo,Consolas,monospace" font-size="14" xml:space="preserve">']
    def rgb(value):
        if len(value) != 3 or any(type(v) is not int or not 0 <= v <= 255 for v in value):
            raise ValueError('Invalid rendered color')
        return '#%02x%02x%02x' % tuple(value)
    for y in range(height):
        visible = []
        next_x = 0
        for cell in snapshot['cells'][y * width:(y + 1) * width]:
            x = cell['x']
            if cell['y'] != y or not 0 <= x < width:
                raise ValueError('Invalid cell coordinate')
            if x < next_x:
                continue
            span = max(1, min(cell['display_width'], width - x))
            next_x = x + span
            visible.append((cell, span))
        backgrounds = []
        for cell, span in visible:
            color = rgb(cell['bg'])
            if backgrounds and backgrounds[-1][2] == color and backgrounds[-1][0] + backgrounds[-1][1] == cell['x']:
                backgrounds[-1][1] += span
            else:
                backgrounds.append([cell['x'], span, color])
        for x, span, color in backgrounds:
            out.append(f'<rect x="{18+x*9}" y="{18+y*21}" width="{span*9}" height="21" fill="{color}"/>')
        for cell, span in visible:
            value = cell['text']
            if not value.strip():
                continue
            if any(ord(c) < 32 for c in value):
                raise ValueError('Control characters cannot enter published illustrations')
            weight = '600' if cell['bold'] else '400'
            out.append(f'<text x="{18+cell["x"]*9}" y="{35+y*21}" fill="{rgb(cell["fg"])}" font-weight="{weight}" textLength="{span*9}" lengthAdjust="spacingAndGlyphs">{escape(value)}</text>')
    out.append('</g></svg>')
    return '\n'.join(out) + '\n'


def validate(directory: Path, root: Path = ROOT) -> dict:
    path = directory / 'illustrations.json'
    manifest = json.loads(path.read_text(encoding='utf-8'))
    if manifest.get('scope') != 'actual_offline_ratatui' or manifest.get('hardware_validation') is not False:
        raise ValueError('Missing explicit illustration provenance')
    for name, expected in manifest['source_sha256'].items():
        if name not in SOURCES or digest(root / name) != expected:
            raise ValueError('UI illustrations are stale; run scripts/docs_assets.py --refresh')
    if set(manifest['source_sha256']) != set(SOURCES):
        raise ValueError('Incomplete illustration source bindings')
    expected_names = {f'{view}-{lang}.svg' for view in VIEWS for lang in LANGUAGES}
    if set(manifest['images']) != expected_names:
        raise ValueError('Missing locale or product illustration')
    for name, info in manifest['images'].items():
        file = directory / name
        if file.is_symlink() or digest(file) != info['sha256']:
            raise ValueError('Missing, linked or modified illustration: ' + name)
        document = ET.fromstring(file.read_text(encoding='utf-8'))
        allowed = {'svg', 'g', 'rect', 'text', 'title', 'desc'}
        for element in document.iter():
            if element.tag.split('}')[-1] not in allowed:
                raise ValueError('Active or foreign SVG content rejected')
            for key, value in element.attrib.items():
                if key.lower().startswith('on') or 'href' in key.lower() or 'url(' in value.lower():
                    raise ValueError('External or executable illustration content rejected')
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh', action='store_true')
    args = parser.parse_args()
    directory = ROOT / 'docs/assets'
    if not args.refresh:
        report = validate(directory)
        print(f'Validated {len(report["images"])} real offline illustrations.')
        return
    logs = []
    # Product illustrations use one reproducible display capability fixture, independent
    # of a caller's shell/CI color and motion preferences. No user settings are changed.
    environment = dict(os.environ, TERM='xterm-256color', COLORTERM='truecolor', MARIS_REDUCED_MOTION='0')
    environment.pop('NO_COLOR', None)
    for flags in (['--locales'], ['--settings', '--draft']):
        result = subprocess.run(['cargo', 'run', '--locked', '--quiet', '--example', 'ui_probe', '--'] + flags,
                                cwd=ROOT, env=environment, capture_output=True, text=True, check=True, timeout=180)
        logs.append(result.stdout + result.stderr)
    manifest = {'scope': 'actual_offline_ratatui', 'hardware_validation': False,
                'source_sha256': {name: digest(ROOT / name) for name in SOURCES}, 'images': {}}
    for view, prefix in VIEWS.items():
        for locale in LANGUAGES:
            suffix = '' if locale == 'en' else '-' + locale
            source = ROOT / '.maris-review' / (prefix + suffix + '.json')
            data = json.loads(source.read_text(encoding='utf-8'))
            output = directory / f'{view}-{locale}.svg'
            output.write_text(export(data), encoding='utf-8', newline='\n')
            manifest['images'][output.name] = {'sha256': digest(output), 'buffer_sha256': digest(source),
                                              'locale': locale, 'view': view, 'columns': 140, 'rows': 40}
    (directory / 'illustrations.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    (ROOT / '.maris-review/docs-capture.log').write_text('\n'.join(logs), encoding='utf-8')
    validate(directory)
    print('Exported 18 actual offline UI illustrations across six languages; no audio capture started.')


if __name__ == '__main__':
    main()
