"""Validate generated HTML, page language, images and project-relative links without a browser."""
from __future__ import annotations
from html.parser import HTMLParser
from pathlib import Path
import posixpath
from urllib.parse import unquote, urlsplit
from docs_assets import LANGUAGES


class Page(HTMLParser):
    def __init__(self, text: str):
        super().__init__(convert_charrefs=True)
        self.links = []
        self.ids = set()
        self.main = self.h1 = self.title = 0
        self.lang = None
        self.viewport = False
        self.images = 0
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag in ('script','iframe','object','embed','base') or any(key.startswith('on') for key in values):
            raise ValueError('Executable or embedded document content rejected')
        if tag == 'html':
            self.lang = values.get('lang')
        self.main += tag == 'main'
        self.h1 += tag == 'h1'
        self.title += tag == 'title'
        self.viewport |= tag == 'meta' and values.get('name') == 'viewport'
        if 'id' in values:
            if values['id'] in self.ids:
                raise ValueError('Duplicate HTML anchor')
            self.ids.add(values['id'])
        for attr in ('href','src'):
            if values.get(attr):
                self.links.append(values[attr])
        if tag == 'img':
            self.images += 1
            if not values.get('alt') or not values.get('width') or not values.get('height'):
                raise ValueError('Images need a useful alt text and explicit dimensions')
        if tag == 'button':
            raise ValueError('No nonfunctional button affordances in a script-free site')


def check(files: dict[str, bytes]) -> list[str]:
    if 'index.html' not in files:
        raise ValueError('Missing homepage')
    pages = {}
    for name, data in files.items():
        if name.startswith('/') or '..' in Path(name).parts or '\\' in name or (Path(name).suffix not in ('.html','.css','.svg') and name not in ('install.sh','install.ps1')):
            raise ValueError('Unexpected generated file: ' + name)
        if name.endswith('.html'):
            page = Page(data.decode('utf-8'))
            if (page.lang not in LANGUAGES or page.main != 1 or page.h1 != 1 or page.title != 1
                    or not page.viewport or 'main' not in page.ids):
                raise ValueError('Missing accessible structure or language: ' + name)
            pages[name] = page
    for name, page in pages.items():
        for link in page.links:
            parsed = urlsplit(link)
            if parsed.scheme or parsed.netloc:
                if parsed.scheme not in ('https', 'mailto'):
                    raise ValueError('Unsafe external link')
                continue
            path = unquote(parsed.path)
            if path.startswith('/') or '\\' in path:
                raise ValueError('Link escapes the project-site prefix')
            target = posixpath.normpath(posixpath.join(posixpath.dirname(name), path)) if path else name
            if target.startswith('../') or target not in files:
                raise ValueError(f'Broken or escaping link in {name}: {link}')
            if target in pages and parsed.fragment and unquote(parsed.fragment) not in pages[target].ids:
                raise ValueError(f'Broken anchor in {name}: {link}')
    return sorted(files)


def directory(path: Path) -> list[str]:
    files = {}
    for file in path.rglob('*'):
        if file.is_symlink():
            raise ValueError('Linked output file rejected')
        if file.is_file() and file.name not in ('.maris-generated','.nojekyll'):
            files[file.relative_to(path).as_posix()] = file.read_bytes()
    return check(files)
