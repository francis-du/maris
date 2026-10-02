"""Build the illustrated documentation site from one Markdown source per page/language."""
from __future__ import annotations
from html import escape, unescape
import json
from pathlib import Path
import posixpath
import re
from urllib.parse import quote, unquote, urlsplit
from docs_assets import LANGUAGES, validate as validate_illustrations
from docs_markdown import render, slug
from release_requirements import load as load_requirements, REQUIRED

GUIDES = ('index', 'guide', 'install', 'status')
BRAND = ('mark.svg', 'mark-mono.svg', 'wordmark.svg')
REFERENCES = {
    'architecture': 'reference/architecture.md', 'audio-quality': 'reference/audio-quality.md',
    'commands': 'reference/commands.md', 'third-party': 'reference/third-party.md',
    'ui-design': 'development/ui-design.md', 'models': 'development/models.md',
    'product-gates': 'development/product-gates.md', 'releasing': 'development/releasing.md',
}
INSTALLERS = ('install.sh', 'install.ps1')
NAMES = {'en': 'English', 'zh-CN': '简体中文', 'zh-TW': '繁體中文', 'ja': '日本語', 'de': 'Deutsch', 'es': 'Español'}
ORIGIN = 'https://francis-du.github.io/maris/'
REPO = 'https://github.com/francis-du/maris'


def relative(target: str, output: str) -> str:
    return posixpath.relpath(target, posixpath.dirname(output) or '.')


def source_path(root: Path, locale: str, page: str) -> Path:
    return root / 'docs' / (REFERENCES[page] if page in REFERENCES else f'{locale}/{page}.md')


def translations(root: Path) -> dict:
    labels = {lang: json.loads((root / 'docs/locales' / f'{lang}.json').read_text(encoding='utf-8')) for lang in LANGUAGES}
    expected = set(labels['en'])
    required = {'tagline','preview','guide','install','status','index','source','skip','language','navigation',
                'on_page','reference','reference_note','offline','zoom','footer','start','explore',
                'flow_title','flow','workflow_title','workflow','pending','in_progress','verified','requirement',
                'state','release_note','edit','top','local','device','reversible','read_more','refs','requirements'}
    if expected != required:
        raise ValueError('Website shell translation schema mismatch')
    for lang, entries in labels.items():
        if set(entries) != expected or any(not value for value in entries.values()):
            raise ValueError('Missing website translation in ' + lang)
        if set(entries['requirements']) != set(REQUIRED) or set(entries['refs']) != set(REFERENCES):
            raise ValueError('Missing translated requirement/reference labels in ' + lang)
        if len(entries['flow']) != 4 or len(entries['workflow']) != 5:
            raise ValueError('Missing translated diagram stages in ' + lang)
    return labels


def resolver(root: Path, origin: Path, locale: str, output: str):
    sources = {source_path(root, lang, page).resolve(): page for lang in LANGUAGES for page in GUIDES + tuple(REFERENCES)}
    def resolve(target: str) -> str:
        parsed = urlsplit(target)
        if parsed.scheme:
            if parsed.scheme not in ('https', 'mailto'):
                raise ValueError('Unsafe documentation link scheme')
            return target
        if parsed.netloc or '\\' in target or unquote(parsed.path).startswith('/'):
            raise ValueError('Absolute or ambiguous documentation link rejected')
        if not parsed.path:
            return '#' + parsed.fragment if parsed.fragment else relative(f'{locale}/index.html', output)
        path = unquote(parsed.path)
        candidate = (origin.parent / path).resolve()
        if candidate in sources:
            result = f'{locale}/{sources[candidate]}.html'
        elif candidate.is_relative_to((root / 'docs/assets').resolve()) and candidate.is_file():
            result = 'assets/' + candidate.name
        elif candidate.is_relative_to(root.resolve()) and candidate.is_file():
            return REPO + '/blob/main/' + quote(candidate.relative_to(root.resolve()).as_posix()) + (('#' + parsed.fragment) if parsed.fragment else '')
        else:
            raise ValueError(f'Unresolved documentation link in {origin.relative_to(root)}: {target}')
        return relative(result, output) + (('#' + parsed.fragment) if parsed.fragment else '')
    return resolve


def nav(locale: str, output: str, page: str, ui: dict, technical: bool = False) -> str:
    items = []
    mapping = REFERENCES if technical else dict.fromkeys(GUIDES)
    for key in mapping:
        label = ui['refs'][key] if technical else ui[key]
        current = ' aria-current="page"' if key == page else ''
        badge = '<span class="language-badge">EN</span>' if technical else ''
        items.append('<a href="' + relative(f'{locale}/{key}.html', output) + '"' + current + '>' + escape(label) + badge + '</a>')
    return ''.join(items)


def diagram(title: str, values: list[str], kind: str) -> str:
    return ('<section class="diagram"><h2>' + escape(title) + '</h2><ol class="' + kind + '">' +
            ''.join(f'<li><span aria-hidden="true">{i+1:02d}</span><strong>{escape(value)}</strong></li>' for i, value in enumerate(values)) + '</ol></section>')


def requirements(root: Path, ui: dict) -> str:
    rows = []
    for item in load_requirements(root):
        key, state = item['id'], item['status']
        rows.append('<tr><th scope="row">' + escape(ui['requirements'][key]) + '</th><td><span class="state ' + state + '">' + escape(ui[state]) + '</span></td></tr>')
    return '<div class="release-notice">' + escape(ui['release_note']) + '</div><div class="table-scroll"><table><thead><tr><th>' + escape(ui['requirement']) + '</th><th>' + escape(ui['state']) + '</th></tr></thead><tbody>' + ''.join(rows) + '</tbody></table></div>'


def page_html(root: Path, locale: str, page: str, labels: dict, version: str, output: str | None = None) -> str:
    output = output or f'{locale}/{page}.html'
    ui = labels[locale]
    origin = source_path(root, locale, page)
    doc = render(origin.read_text(encoding='utf-8'), resolver(root, origin, locale, output), ui['offline'], ui['zoom'])
    technical = page in REFERENCES
    home = page == 'index'
    target = lambda slug: relative(f'{locale}/{slug}.html', output)
    language_links = ''.join('<a href="' + relative(f'{lang}/{page}.html', output) + '" lang="' + lang + '" hreflang="' + lang + '"' + (' aria-current="page"' if locale == lang else '') + '>' + name + '</a>' for lang, name in NAMES.items())
    alternates = ''.join('<link rel="alternate" hreflang="' + lang + '" href="' + ORIGIN + lang + '/' + page + '.html">' for lang in LANGUAGES)
    header = ('<header class="site-header"><div class="header-inner"><a class="brand" href="' + target('index') + '" aria-label="Maris"><img class="brand-mark" src="' + relative('assets/mark.svg', output) + '" width="32" height="32" alt="Maris">MARIS</a>' +
              '<nav class="top-nav" aria-label="' + escape(ui['navigation']) + '">' + nav(locale, output, page, ui) + '</nav>' +
              '<details class="languages"><summary aria-label="' + escape(ui['language']) + '">' + NAMES[locale] + ' <span aria-hidden="true">⌄</span></summary><nav aria-label="' + escape(ui['language']) + '">' + language_links + '</nav></details>' +
              '<a class="source-link" href="' + REPO + '">' + escape(ui['source']) + ' ↗</a></div></header>')
    preview = '<a class="preview-label" href="' + target('status') + '"><span aria-hidden="true">○</span> v' + version + ' · ' + escape(ui['preview']) + '</a>'
    title = '<div class="eyebrow">' + escape(ui['tagline']) + '</div><h1' + (' lang="en"' if technical else '') + '>' + escape(doc['title']) + '</h1>'
    lead = '<div class="lead"' + (' lang="en"' if technical else '') + '>' + doc['lead'] + '</div>'
    content = doc['body']
    if home:
        hero = '<section class="hero">' + preview + title + lead + '<div class="hero-actions"><a class="button primary" href="' + target('install') + '">' + escape(ui['start']) + ' <span aria-hidden="true">↗</span></a><a class="button" href="' + target('guide') + '">' + escape(ui['explore']) + ' →</a></div><div class="traits">' + ''.join('<span>' + escape(ui[key]) + '</span>' for key in ('local','device','reversible')) + '</div></section>'
        body = '<main id="main" class="home" tabindex="-1">' + hero + '<div class="hero-screen">' + doc['hero'] + '</div>' + diagram(ui['flow_title'], ui['flow'], 'signal-flow') + '<article class="home-article">' + content + '</article>' + diagram(ui['workflow_title'], ui['workflow'], 'workflow') + '<section class="closing"><p>' + escape(ui['release_note']) + '</p><a class="button" href="' + target('status') + '">' + escape(ui['status']) + ' →</a></section></main>'
    else:
        reference_nav = '<nav aria-label="' + escape(ui['reference']) + '">' + nav(locale, output, page, ui, True) + '</nav>'
        sidebar = '<aside class="sidebar"><nav aria-label="' + escape(ui['navigation']) + '">' + nav(locale, output, page, ui) + '</nav><div class="desktop-reference"><div class="sidebar-label">' + escape(ui['reference']) + '</div>' + reference_nav + '</div><details class="mobile-reference"><summary>' + escape(ui['reference']) + ' <span class="language-badge">EN</span></summary>' + reference_nav + '</details></aside>'
        toc = '<aside class="toc"><div>' + escape(ui['on_page']) + '</div><nav aria-label="' + escape(ui['on_page']) + '">' + ''.join('<a href="#' + escape(anchor) + '"' + (' lang="en"' if technical else '') + '>' + escape(text) + '</a>' for anchor, text in doc['toc']) + '</nav></aside>'
        note = '<div class="reference-notice">' + escape(ui['reference_note']) + '</div>' if technical else ''
        if page == 'status':
            content = requirements(root, ui) + content
        article = '<main id="main" class="document" tabindex="-1"><div class="doc-heading">' + preview + title + lead + '</div>' + note + '<article' + (' lang="en"' if technical else '') + '>' + doc['hero'] + content + '</article><div class="document-end"><a href="' + REPO + '/blob/main/' + origin.relative_to(root).as_posix() + '">' + escape(ui['edit']) + ' ↗</a><a href="#main">' + escape(ui['top']) + ' ↑</a></div></main>'
        body = '<div class="docs-layout">' + sidebar + article + toc + '</div>'
    footer = '<footer class="site-footer"><a class="brand" href="' + target('index') + '">MARIS</a><p>' + escape(ui['footer']) + '</p><a href="' + REPO + '">' + escape(ui['source']) + ' ↗</a><a href="' + target('status') + '">' + escape(ui['status']) + ' →</a></footer>'
    description = unescape(re.sub('<[^>]+>', '', doc['lead']))[:180]
    return ('<!doctype html>\n<html lang="' + locale + '"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">' +
            '<meta name="color-scheme" content="dark"><meta name="description" content="' + escape(description, quote=True) + '"><title>' + escape(doc['title']) + ' — Maris</title><link rel="canonical" href="' + ORIGIN + locale + '/' + page + '.html">' + alternates +
            '<link rel="icon" type="image/svg+xml" href="' + relative('assets/mark.svg', output) + '"><link rel="stylesheet" href="' + relative('assets/site.css', output) + '"></head><body><a class="skip" href="#main">' + escape(ui['skip']) + '</a>' + header + body + footer + '</body></html>\n')


def check_source_links(root: Path) -> None:
    """Check Markdown targets before generated-page rewriting can hide a bad source link."""
    documents = [root / 'README.md', root / 'AGENTS.md', *sorted((root / 'docs').rglob('*.md'))]
    pattern = re.compile(r'!?\[[^\]\n]*\]\(([^\s)]+)\)')
    for origin in documents:
        text = origin.read_text(encoding='utf-8')
        for target in pattern.findall(text):
            parsed = urlsplit(target)
            if parsed.scheme:
                if parsed.scheme not in ('https', 'mailto'):
                    raise ValueError('Unsupported document link scheme')
                continue
            if parsed.netloc or '\\' in target or unquote(parsed.path).startswith('/'):
                raise ValueError('Absolute document path rejected')
            path = (origin.parent / unquote(parsed.path)).resolve() if parsed.path else origin
            if not path.is_relative_to(root) or not path.is_file() or path.is_symlink():
                raise ValueError(f'Broken source link in {origin.relative_to(root)}: {target}')
            if parsed.fragment and path.suffix == '.md':
                headings = re.findall(r'^#{1,6} (.+)$', path.read_text(encoding='utf-8'), re.M)
                anchors = set()
                for heading in headings:
                    explicit = re.search(r'\s+\{#([\w-]+)\}$', heading)
                    anchors.add(explicit[1] if explicit else slug(heading))
                if unquote(parsed.fragment) not in anchors:
                    raise ValueError(f'Broken source anchor in {origin.relative_to(root)}: {target}')


def build(root: Path) -> dict[str, bytes]:
    root = root.resolve()
    check_source_links(root)
    labels = translations(root)
    assets = validate_illustrations(root / 'docs/assets', root)
    version_match = re.search(r'^version = "([0-9.]+)"$', (root / 'Cargo.toml').read_text(encoding='utf-8'), re.M)
    if version_match is None:
        raise ValueError('Missing product version')
    files = {}
    for locale in LANGUAGES:
        for page in GUIDES + tuple(REFERENCES):
            files[f'{locale}/{page}.html'] = page_html(root, locale, page, labels, version_match[1]).encode('utf-8')
    files['index.html'] = page_html(root, 'en', 'index', labels, version_match[1], 'index.html').encode('utf-8')
    # Keep original public URLs working without maintaining duplicate HTML articles.
    for old, page in {'guide.html':'guide','install.html':'install','architecture.html':'architecture','releasing.html':'releasing'}.items():
        files[old] = page_html(root, 'en', page, labels, version_match[1], old).encode('utf-8')
    for name in INSTALLERS:
        file = root / name
        if file.is_symlink() or not file.is_file():
            raise ValueError('Missing or linked installer source')
        files[name] = file.read_bytes()
    for name in ['site.css'] + list(BRAND) + list(assets['images']):
        files['assets/' + name] = (root / 'docs/assets' / name).read_bytes()
    expected = {source_path(root, lang, page).resolve() for lang in LANGUAGES for page in GUIDES + tuple(REFERENCES)}
    expected |= {(root / 'docs/locales' / f'{lang}.json').resolve() for lang in LANGUAGES}
    expected |= {(root / 'docs/assets' / name).resolve() for name in ['site.css','illustrations.json'] + list(BRAND) + list(assets['images'])}
    expected |= {(root / 'docs/README.md').resolve(), (root / 'docs/development/requirements.json').resolve()}
    for path in (root / 'docs').rglob('*'):
        if path.is_symlink() or (path.is_file() and path.resolve() not in expected):
            raise ValueError('Unexpected documentation source: ' + path.relative_to(root).as_posix())
    return files
