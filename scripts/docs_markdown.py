"""Small, explicit documentation Markdown subset; raw HTML is always escaped.

Supported: headings/explicit anchors, paragraphs, fenced code, lists, tables,
blockquotes, inline emphasis/code/links, and standalone product illustrations.
This is deliberately not a CommonMark implementation or an HTML passthrough.
"""
from __future__ import annotations
from html import escape
import re
import unicodedata

TOKEN = re.compile(r'(`[^`\n]+`|\*\*[^*\n]+\*\*|\*[^*\n]+\*|\[[^\]\n]+\]\([^\s)]+\)|https://[^\s<>]+)')
IMAGE = re.compile(r'^!\[([^\]]+)\]\(([^\s)]+)\)$')
HEADING = re.compile(r'^(#{1,4})\s+(.+?)(?:\s+\{#([a-zA-Z][\w-]*)\})?$')


def inline(text: str, resolve) -> str:
    pieces = []
    end = 0
    for match in TOKEN.finditer(text):
        pieces.append(escape(text[end:match.start()]))
        value = match.group()
        if value.startswith('`'):
            pieces.append('<code>' + escape(value[1:-1]) + '</code>')
        elif value.startswith('**'):
            pieces.append('<strong>' + inline(value[2:-2], resolve) + '</strong>')
        elif value.startswith('*'):
            pieces.append('<em>' + inline(value[1:-1], resolve) + '</em>')
        elif value.startswith('['):
            label, target = value[1:].split('](', 1)
            pieces.append('<a href="' + escape(resolve(target[:-1]), quote=True) + '">' + escape(label) + '</a>')
        else:
            value = value.rstrip('.,;')
            pieces.append('<a href="' + escape(resolve(value), quote=True) + '">' + escape(value) + '</a>')
            pieces.append(escape(match.group()[len(value):]))
        end = match.end()
    pieces.append(escape(text[end:]))
    return ''.join(pieces)


def slug(text: str) -> str:
    normalized = unicodedata.normalize('NFKC', re.sub(r'[`*]', '', text)).lower()
    return re.sub(r'[^\w-]+', '-', normalized, flags=re.UNICODE).strip('-') or 'section'


def render(text: str, resolve, caption: str, zoom: str) -> dict:
    lines = text.splitlines()
    if not lines or not lines[0].startswith('# '):
        raise ValueError('Documentation must start with one H1 title')
    title = lines[0][2:].strip()
    body, toc, ids = [], [], set()
    lead, first_image = '', ''
    i = 1
    section = False
    while i < len(lines):
        line = lines[i].strip()
        if not line:
            i += 1
            continue
        if line.startswith('```'):
            language = line[3:].strip()
            if not re.fullmatch(r'[\w+-]*', language):
                raise ValueError('Invalid code language')
            code = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith('```'):
                code.append(lines[i])
                i += 1
            if i == len(lines):
                raise ValueError('Unclosed code fence')
            body.append('<pre><code class="language-' + escape(language) + '">' + escape('\n'.join(code)) + '</code></pre>')
            i += 1
            continue
        heading = HEADING.fullmatch(line)
        if heading:
            level, label, anchor = len(heading[1]), heading[2], heading[3]
            if level == 1:
                raise ValueError('Only one H1 is allowed')
            base = anchor or slug(label)
            anchor = base
            n = 2
            while anchor in ids:
                if heading[3]:
                    raise ValueError('Duplicate explicit anchor: ' + anchor)
                anchor = base + '-' + str(n)
                n += 1
            ids.add(anchor)
            if level == 2:
                if section:
                    body.append('</section>')
                body.append('<section class="doc-section">')
                section = True
                toc.append((anchor, label))
            body.append(f'<h{level} id="{escape(anchor)}">{inline(label, resolve)}</h{level}>')
            i += 1
            continue
        image = IMAGE.fullmatch(line)
        if image:
            target = resolve(image[2])
            if not target.endswith('.svg'):
                raise ValueError('Only reviewed local SVG illustrations are supported')
            figure = ('<figure class="product-shot"><a href="' + escape(target, quote=True) + '"><img src="' +
                      escape(target, quote=True) + '" alt="' + escape(image[1], quote=True) +
                      '" width="1296" height="876" loading="lazy" decoding="async"></a><figcaption>' +
                      escape(caption) + ' · <a href="' + escape(target, quote=True) + '">' + escape(zoom) + '</a></figcaption></figure>')
            if not first_image and not section:
                first_image = figure.replace('loading="lazy"', 'loading="eager" fetchpriority="high"')
            else:
                body.append(figure)
            i += 1
            continue
        if line.startswith('> '):
            quoted = []
            while i < len(lines) and lines[i].strip().startswith('> '):
                quoted.append(lines[i].strip()[2:])
                i += 1
            body.append('<blockquote><p>' + inline(' '.join(quoted), resolve) + '</p></blockquote>')
            continue
        if line.startswith('|') and i + 1 < len(lines) and re.fullmatch(r'[| :\-]+', lines[i + 1].strip()):
            rows = []
            while i < len(lines) and lines[i].strip().startswith('|'):
                rows.append([cell.strip().replace('\\|', '|') for cell in re.split(r'(?<!\\)\|', lines[i].strip().strip('|'))])
                i += 1
            if len(rows) < 2 or not rows[0]:
                raise ValueError('Malformed Markdown table')
            count = len(rows[0])
            if any(len(row) != count for row in rows):
                raise ValueError('Inconsistent Markdown table columns')
            body.append('<div class="table-scroll" tabindex="0"><table><thead><tr>' + ''.join('<th scope="col">' + inline(c, resolve) + '</th>' for c in rows[0]) + '</tr></thead><tbody>')
            for row in rows[2:]:
                body.append('<tr>' + ''.join('<td>' + inline(c, resolve) + '</td>' for c in row) + '</tr>')
            body.append('</tbody></table></div>')
            continue
        listing = re.match(r'^(?:[-*] |[0-9]+\. )(.+)$', line)
        if listing:
            ordered = line[0].isdigit()
            tag = 'ol' if ordered else 'ul'
            items = []
            pattern = r'^[0-9]+\. (.+)$' if ordered else r'^[-*] (.+)$'
            while i < len(lines):
                current = re.match(pattern, lines[i].strip())
                if current is None:
                    break
                value = current[1]
                i += 1
                while i < len(lines) and lines[i].startswith('  ') and lines[i].strip():
                    value += ' ' + lines[i].strip()
                    i += 1
                items.append('<li>' + inline(value, resolve) + '</li>')
            body.append('<' + tag + '>' + ''.join(items) + '</' + tag + '>')
            continue
        if line == '---':
            body.append('<hr>')
            i += 1
            continue
        paragraph = [line]
        i += 1
        while i < len(lines) and lines[i].strip() and not re.match(r'^(#|```|>|\||!\[|[-*] |\d+\. )', lines[i].strip()):
            paragraph.append(lines[i].strip())
            i += 1
        value = inline(' '.join(paragraph), resolve)
        if not lead and not body:
            lead = value
        else:
            body.append('<p>' + value + '</p>')
    if section:
        body.append('</section>')
    return {'title': title, 'lead': lead, 'body': '\n'.join(body), 'toc': toc, 'hero': first_image}
