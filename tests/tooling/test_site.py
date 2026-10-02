"""Canonical documentation, translated navigation, image provenance and safe HTML regressions."""
from html.parser import HTMLParser
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from docs_assets import LANGUAGES, validate as validate_images
from capture_menu_docs import validate as validate_native_images
from docs_build import BRAND, GUIDES, REFERENCES, build, resolver, translations
from docs_check import check as check_output
from docs_markdown import render
from release_requirements import REQUIRED, load as load_requirements
SPEC = importlib.util.spec_from_file_location('maris_site_tests', ROOT / 'scripts/site.py')
site = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(site)


class Links(HTMLParser):
    def __init__(self, text):
        super().__init__(); self.links=[]; self.images=[]; self.feed(text)
    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == 'a': self.links.append(values)
        if tag == 'img': self.images.append(values)


class Documentation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.output = build(ROOT)

    def test_one_source_tree_produces_all_languages_and_legacy_urls(self):
        self.assertEqual(len(check_output(self.output)), 109)
        for locale in LANGUAGES:
            for page in GUIDES:
                data = self.output[f'{locale}/{page}.html'].decode()
                self.assertIn(f'<html lang="{locale}">', data)
                self.assertEqual(data.count('<h1'), 1)
        for old in ('index.html','install.html','guide.html','architecture.html','releasing.html'):
            self.assertIn(old, self.output)
        self.assertEqual({p.name for p in ROOT.glob('*.md')}, {'README.md','AGENTS.md'})
        self.assertFalse(list((ROOT / 'docs').glob('*.html')))

    def test_language_switch_preserves_current_page_and_all_images_have_alt(self):
        for locale in LANGUAGES:
            for page in ('install','guide','architecture'):
                parsed = Links(self.output[f'{locale}/{page}.html'].decode())
                switches = [link for link in parsed.links if 'hreflang' in link]
                self.assertEqual({link['hreflang'] for link in switches}, set(LANGUAGES))
                self.assertTrue(all(link['href'].endswith('/' + page + '.html') or link['href'] == page + '.html' for link in switches))
                self.assertTrue(all(image.get('alt') and image.get('width') and image.get('height') for image in parsed.images))

    def test_engineering_english_is_labeled_not_claimed_translated(self):
        labels = translations(ROOT)
        for locale in LANGUAGES:
            text = self.output[f'{locale}/architecture.html'].decode()
            self.assertIn(labels[locale]['reference_note'], text)
            self.assertIn('<article lang="en">', text)
            for page in GUIDES:
                self.assertNotIn('<article lang="en">', self.output[f'{locale}/{page}.html'].decode())

    def test_missing_translation_or_previous_requirement_cannot_disappear(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copytree(ROOT / 'docs/locales', root / 'docs/locales')
            file = root / 'docs/locales/ja.json'
            data = json.loads(file.read_text(encoding='utf-8')); del data['requirements']['brand-readme']; file.write_text(json.dumps(data))
            with self.assertRaises(ValueError): translations(root)
            path = root / 'docs/development/requirements.json'; path.parent.mkdir()
            records = [{'id': key, 'status':'pending'} for key in REQUIRED[:-1]]
            path.write_text(json.dumps({'schema_version':1,'policy':'all_requested_work_before_release','requirements':records}))
            with self.assertRaises(ValueError): load_requirements(root)

    def test_real_illustrations_are_complete_source_bound_and_non_hardware(self):
        manifest = validate_images(ROOT / 'docs/assets')
        self.assertEqual(len(manifest['images']), 18)
        self.assertFalse(manifest['hardware_validation'])
        self.assertEqual({value['locale'] for value in manifest['images'].values()}, set(LANGUAGES))
        self.assertTrue(all('buffer_sha256' in value for value in manifest['images'].values()))

    def test_native_menu_views_are_source_bound_and_published_in_both_review_languages(self):
        manifest = validate_native_images(ROOT / 'docs/assets')
        self.assertEqual(set(manifest['images']), {'menu-bar-en.svg', 'menu-bar-zh-CN.svg'})
        self.assertFalse(manifest['hardware_validation'])
        self.assertFalse(manifest['audio_started'])
        self.assertFalse(manifest['whole_desktop_captured'])
        for name, info in manifest['images'].items():
            self.assertIn('assets/' + name, self.output)
            self.assertEqual(len(info['captures']), 16)

    def test_logo_is_consistent_vector_geometry_without_remote_fonts_or_scripts(self):
        paths = []
        for name in BRAND:
            document = ET.parse(ROOT / 'docs/assets' / name)
            path = document.find('.//{http://www.w3.org/2000/svg}path')
            self.assertIsNotNone(path); paths.append(path.attrib['d'])
            for node in document.iter():
                self.assertNotIn(node.tag.split('}')[-1], ('script','foreignObject','image'))
                self.assertFalse(any('href' in key or key.startswith('on') for key in node.attrib))
        self.assertEqual(len(set(paths)), 1)
        self.assertIn('assets/mark.svg', self.output['index.html'].decode())
        self.assertIn('rel="icon"', self.output['index.html'].decode())

    def test_published_wordmark_explains_actions_and_readme_does_not_claim_a_release(self):
        namespace = {'svg': 'http://www.w3.org/2000/svg'}
        wordmark = ET.fromstring(self.output['assets/wordmark.svg'])
        labels = [node.text for node in wordmark.findall('svg:text', namespace)]
        self.assertEqual(labels, ['MARIS', 'ADJUST YOUR SOUND · COMPARE AND UNDO'])
        description = wordmark.find('svg:desc', namespace)
        self.assertIsNotNone(description)
        self.assertIn('Adjust your sound, compare and undo.', description.text)
        readme = (ROOT / 'README.md').read_text(encoding='utf-8')
        self.assertIn('A public application release is not available yet.', readme)
        self.assertIn('does not include its weights', readme)

    def test_renderer_escapes_raw_html_and_validates_links_and_structure(self):
        resolve = resolver(ROOT, ROOT / 'docs/en/guide.md', 'en', 'en/guide.html')
        value = render('# Title\n\n<script>alert(1)</script>\n\n## Safe\n\n**Bold** and `code`.\n', resolve, 'Fixture', 'Zoom')
        self.assertIn('&lt;script&gt;', value['lead'])
        self.assertNotIn('<script>', value['lead'])
        self.assertIn('<strong>Bold</strong>', value['body'])
        for target in ('javascript:alert(1)', '//example.com/file', '/escape', '../../outside-missing.md'):
            with self.assertRaises(ValueError): resolve(target)
        for text in ('No title', '# A\n\n# B', '# A\n\n```rust\nunclosed', '# A\n\n## X {#same}\n\n## Y {#same}'):
            with self.assertRaises(ValueError): render(text, resolve, 'Fixture', 'Zoom')

    def test_generated_links_reject_escape_missing_anchor_and_active_content(self):
        text = '<html lang="en"><head><title>Fixture</title><meta name="viewport"></head><body><main id="main"><h1>Fixture</h1>{}</main></body></html>'
        for insert in ('<a href="/escape">x</a>', '<a href="../escape">x</a>', '<a href="#missing">x</a>', '<script></script>', '<img src="x.svg">'):
            with self.assertRaises(ValueError): check_output({'index.html': text.format(insert).encode()})
        self.assertEqual(check_output({'index.html': text.format('<a href="#main">x</a>').encode()}), ['index.html'])

    def test_stage_refuses_unknown_destination_and_retains_known_previous(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); output = root / '_site'; output.mkdir()
            (output / 'keep.txt').write_text('unrelated')
            with self.assertRaises(ValueError): site.stage(self.output, root)
            self.assertEqual((output / 'keep.txt').read_text(encoding='utf-8'), 'unrelated')
            self.assertFalse((root / '.maris-review/site-build.lock').exists())
            (output / '.maris-generated').write_text(site.MARKER)
            site.stage(self.output, root)
            retained = list((root / '.maris-review').glob('site-previous-*'))
            self.assertEqual(len(retained), 1)
            self.assertEqual((retained[0] / 'keep.txt').read_text(encoding='utf-8'), 'unrelated')
            self.assertTrue((output / 'zh-CN/install.html').is_file())

    def test_every_install_language_documents_download_before_developer_build(self):
        for locale in LANGUAGES:
            text = (ROOT / f'docs/{locale}/install.md').read_text(encoding='utf-8')
            self.assertIn('curl -fsSL https://francis-du.github.io/maris/install.sh | bash', text)
            self.assertIn('irm https://francis-du.github.io/maris/install.ps1 | iex', text)
            self.assertLess(text.index('{#online}'), text.index('{#development}'))
            for option in ('--dry-run', '--version', '-Version', 'SHA-256'):
                self.assertIn(option, text)
        readme = (ROOT / 'README.md').read_text(encoding='utf-8')
        self.assertIn('curl -fsSL https://francis-du.github.io/maris/install.sh | bash', readme)
        self.assertIn('irm https://francis-du.github.io/maris/install.ps1 | iex', readme)


if __name__ == '__main__':
    unittest.main()
