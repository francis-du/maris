#!/usr/bin/env python3
"""Publish actual isolated AppKit menu captures as source-bound UTF-8 SVGs.

Refresh explicitly runs the production offline menu probe. Validation never opens
a menu, starts audio, reads user telemetry or needs macOS. PNG bytes are embedded
unchanged; they are not reconstructed from menu labels or repainted mockups.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
from html import escape
import json
import os
from pathlib import Path
import struct
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
LOCALES = ('en', 'zh-CN')
STATES = ('live', 'idle', 'stale', 'failed', 'stopping', 'restore-failed')
SOURCES = ('Cargo.toml', 'Cargo.lock', 'src/ui/desktop/native.rs',
           'src/ui/desktop/native/menu_review.rs', 'src/ui/desktop/native/native_loop.rs', 'src/ui/desktop/menu_header.rs',
           'src/ui/desktop/native_mark.rs', 'src/ui/desktop/status_icon.rs',
           'src/ui/desktop/menu_capture.rs', 'src/ui/desktop/monitor_state.rs',
           'src/ui/desktop/controls.rs', 'src/ui/desktop/events.rs', 'src/ui/desktop/mod.rs',
           'src/i18n/mod.rs', 'src/i18n/console.rs', 'src/i18n/surface.rs',
           'src/i18n/messages.rs', 'tests/support/menu_probe.rs', 'scripts/capture_menu_docs.py')
MANIFEST = 'menu-bar-review-manifest.json'
LIMIT = 2_000_000
LABELS = {
    'en': ('Maris · native menu', 'Light · processing', 'Dark · processing',
           'Actual offline AppKit views. No audio capture or hardware validation.',
           'Processing', 'Standby', 'Awaiting telemetry', 'Audio unavailable',
           'Stopping audio', 'Restore failed'),
    'zh-CN': ('Maris · 原生菜单', '浅色 · 正在处理', '深色 · 正在处理',
              '真实离线 AppKit 视图；未启动音频采集，未验证硬件。',
              '正在处理', '待机', '等待遥测', '音频不可用', '正在停止音频', '恢复失败'),
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_hashes(root: Path) -> dict[str, str]:
    root = root.resolve()
    hashes = {}
    for name in SOURCES:
        path = root / name
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
            raise ValueError('Missing or linked native render source: ' + name)
        hashes[name] = digest(path.read_bytes())
    return hashes


def png_dimensions(data: bytes) -> tuple[int, int]:
    if not 33 <= len(data) <= LIMIT or data[:16] != b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR':
        raise ValueError('Invalid or oversized native PNG')
    width, height = struct.unpack('>II', data[16:24])
    if not 0 < width <= 4096 or not 0 < height <= 4096:
        raise ValueError('Invalid native PNG dimensions')
    return width, height


def image_source(root: Path, item: dict, kind: str) -> dict:
    root = root.resolve()
    name = item[kind + '_path']
    path = Path(name)
    if path.is_absolute() or '..' in path.parts or not name.startswith('.maris-review/native-menu/'):
        raise ValueError('Capture must belong to the isolated native probe')
    file = root / path
    if file.is_symlink() or not file.resolve().is_relative_to(root / '.maris-review/native-menu'):
        raise ValueError('Linked or escaping capture')
    data = file.read_bytes()
    width, height = png_dimensions(data)
    info = item[kind]
    if (info.get('whole_desktop_captured') is not False
            or info.get('renderer') != 'AppKit cacheDisplayInRect:toBitmapImageRep:'
            or (info.get('pixel_width'), info.get('pixel_height')) != (width, height)
            or info.get('png_bytes') != len(data) or info.get('appearance') != item['appearance']):
        raise ValueError('Capture provenance or PNG metadata does not match')
    if kind == 'bar' and not (18 <= info['logical_width'] <= 64 and 18 <= info['logical_height'] <= 64):
        raise ValueError('Status button is not a single native menu-bar row')
    return {'data': data, 'sha256': digest(data), 'pixel_width': width,
            'pixel_height': height, 'capture': info, 'path': name}


def captures(root: Path, report: dict) -> dict:
    if (report.get('audio_started') is not False or report.get('hardware_validated') is not False
            or report.get('source') != 'isolated offline fixture'
            or report.get('native_menu_construction') is not True):
        raise ValueError('Requires the explicitly isolated production native-menu probe')
    result = {}
    for locale in report['locales']:
        language = locale['language']
        if language not in LOCALES:
            continue
        entries = {}
        for item in locale['native_states']['captures']:
            key = (item['state'], item['appearance'])
            if key in entries or item['language'] != language:
                raise ValueError('Duplicate or cross-locale capture')
            mark = item['image']
            if (mark.get('template') is not True or mark.get('logical_width') != 18
                    or mark.get('logical_height') != 18
                    or not any(rep == {'width': 64, 'height': 64} for rep in mark['representations'])
                    or not mark.get('accessibility_label') or not mark.get('accessibility_value')):
                raise ValueError('Actual 18-point template mark or accessibility metadata is missing')
            entries[key] = item
        required = {('live', 'light'), ('live', 'dark')} | {(state, 'dark') for state in STATES}
        if not required.issubset(entries):
            raise ValueError('Missing native normal/pending/error-state views for ' + language)
        result[language] = entries
    if set(result) != set(LOCALES):
        raise ValueError('Both English and simplified Chinese native captures are required')
    return result


def illustration(root: Path, language: str, entries: dict) -> tuple[str, list[dict]]:
    labels = LABELS[language]
    assets = []
    out = ['<!-- @generated from unchanged production AppKit PNGs; use capture_menu_docs.py refresh mode -->',
           '<svg xmlns="http://www.w3.org/2000/svg" width="1100" height="1300" viewBox="0 0 1100 1300" role="img" aria-labelledby="title desc">',
           '<title id="title">' + escape(labels[0]) + '</title>',
           '<desc id="desc">' + escape(labels[3]) + ' Source-bound offline fixture; 18-point template mark with a 64-pixel native representation.</desc>',
           '<rect width="1100" height="1300" fill="#101317"/>',
           '<g font-family="system-ui,sans-serif" fill="#edf1f3">',
           '<text x="36" y="48" font-size="27" font-weight="650">' + escape(labels[0]) + '</text>',
           '<text x="36" y="78" font-size="16" fill="#acb8c2">' + escape(labels[3]) + '</text>']

    def draw(item: dict, kind: str, x: int, y: int, width: int, height: int):
        source = image_source(root, item, kind)
        encoded = base64.b64encode(source.pop('data')).decode('ascii')
        if kind == 'bar' and item['appearance'] == 'light':
            # A transparent light-appearance status button needs a light page
            # backing, just as it does in the native menu bar. Pixels stay intact.
            out.append(f'<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="5" fill="#f4f5f6"/>')
        out.append(f'<image x="{x}" y="{y}" width="{width}" height="{height}" preserveAspectRatio="xMinYMin meet" href="data:image/png;base64,{encoded}"/>')
        assets.append(dict(source, language=language, state=item['state'], appearance=item['appearance'],
                           kind=kind, image=item['image']))

    for index, key in enumerate((('live', 'light'), ('live', 'dark'))):
        x = 36 + index * 528
        out.append(f'<text x="{x}" y="118" font-size="19" font-weight="600">{escape(labels[1 + index])}</text>')
        item = entries[key]
        draw(item, 'bar', x, 136, 38, 36)
        draw(item, 'menu', x, 181, 492, 678)
    for index, state in enumerate(STATES):
        x, y = 36 + (index % 2) * 528, 901 + (index // 2) * 128
        item = entries[(state, 'dark')]
        out.append(f'<text x="{x}" y="{y}" font-size="17" font-weight="600">{escape(labels[4 + index])}</text>')
        draw(item, 'bar', x, y + 12, 40, 40)
        draw(item, 'header', x + 52, y + 12, 430, 88)
    out.append('</g></svg>')
    return '\n'.join(out) + '\n', assets


def validate(directory: Path, root: Path = ROOT) -> dict:
    root = root.resolve()
    manifest = json.loads((directory / MANIFEST).read_text(encoding='utf-8'))
    if (manifest.get('scope') != 'actual_offline_appkit' or manifest.get('hardware_validation') is not False
            or manifest.get('audio_started') is not False or manifest.get('whole_desktop_captured') is not False
            or manifest.get('source_sha256') != source_hashes(root)):
        raise ValueError('Native menu illustrations are stale or have missing provenance; explicitly refresh them')
    if set(manifest['images']) != {f'menu-bar-{language}.svg' for language in LOCALES}:
        raise ValueError('Incomplete native menu documentation locales')
    for name, info in manifest['images'].items():
        path = directory / name
        data = path.read_bytes()
        if path.is_symlink() or len(data) >= LIMIT or digest(data) != info['sha256']:
            raise ValueError('Linked, oversized or modified native menu illustration')
        document = ET.fromstring(data.decode('utf-8'))
        embedded = []
        for element in document.iter():
            tag = element.tag.split('}')[-1]
            if tag not in {'svg', 'g', 'rect', 'text', 'title', 'desc', 'image'}:
                raise ValueError('Active or foreign SVG content rejected')
            for key, value in element.attrib.items():
                if key.lower().startswith('on') or 'url(' in value.lower():
                    raise ValueError('Executable illustration content rejected')
                if 'href' in key.lower():
                    if tag != 'image' or not value.startswith('data:image/png;base64,'):
                        raise ValueError('External illustration content rejected')
                    png = base64.b64decode(value.split(',', 1)[1], validate=True)
                    png_dimensions(png)
                    embedded.append(digest(png))
        if embedded != [item['sha256'] for item in info['captures']]:
            raise ValueError('Embedded native images differ from the capture manifest')
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh', action='store_true', help='Run the isolated production probe before exporting')
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--output-dir', type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    directory = args.output_dir or root / 'docs/assets'
    if not args.refresh:
        report = validate(directory, root)
        print(f'Validated {len(report["images"])} actual native menu illustrations; no UI or audio started.')
        return
    before = source_hashes(root)
    environment = dict(os.environ, CARGO_BUILD_JOBS='2')
    subprocess.run(['cargo', 'run', '--locked', '--quiet', '--example', 'menu_probe', '--', '--capture'], cwd=root,
                   env=environment, check=True, timeout=240, stdout=subprocess.DEVNULL)
    if source_hashes(root) != before:
        raise ValueError('Native render sources changed during the probe; rerun after source ownership is settled')
    source = root / '.maris-review/menu-review.json'
    report = json.loads(source.read_text(encoding='utf-8'))
    native = captures(root, report)
    manifest = {'scope': 'actual_offline_appkit', 'hardware_validation': False,
                'audio_started': False, 'whole_desktop_captured': False, 'source_sha256': before,
                'probe_sha256': digest(source.read_bytes()), 'images': {}}
    directory.mkdir(parents=True, exist_ok=True)
    for language, entries in native.items():
        document, assets = illustration(root, language, entries)
        data = document.encode('utf-8')
        if len(data) >= LIMIT:
            raise ValueError('Native illustration exceeds the bounded source-file size')
        path = directory / f'menu-bar-{language}.svg'
        path.write_bytes(data)
        manifest['images'][path.name] = {'sha256': digest(data), 'locale': language,
                                        'width': 1100, 'height': 1300, 'captures': assets}
    (directory / MANIFEST).write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    validate(directory, root)
    print('Exported actual production AppKit menu, button and state-header views in two languages; no audio started.')


if __name__ == '__main__':
    main()
