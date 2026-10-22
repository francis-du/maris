"""Keep Rust source grouped by responsibility and every module connected to the crate."""
from pathlib import Path
import hashlib
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'src'
DOMAINS = {'analysis', 'audio', 'cli', 'control', 'devices', 'dsp', 'i18n', 'mixer', 'models', 'presets', 'tuning', 'ui'}
MODULE = re.compile(r'(?m)((?:^[ \t]*#\[[^\n]+\][ \t]*\n)*)(^[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;)')


# Preserve the exact reviewed upstream crate layout without expanding Maris's
# maintained-source limit. These files are byte-identical to candle-core 0.11.0;
# any local edit or new oversized file must receive its own explicit review.
UPSTREAM_LINE_BASELINES = {
    'third_party/candle-core/src/cpu_backend/mod.rs':
        'b3db97cf7d445efc3f7269e14d970f61bb9a6f25838159ed6ea7b94d924fde86',
    'third_party/candle-core/src/cuda_backend/mod.rs':
        '117080fe4e00891701848073bcd45d94211a181f580d8719a1045ecc4212472e',
    'third_party/candle-core/src/metal_backend/mod.rs':
        '015ef2c128a54ea4fb2cdb5705ee26101509de7e17182e4877fc50de1b167758',
    'third_party/candle-core/src/op.rs':
        '3d871e682e2d702e82c0593e222217b90812d7e953a6f97fb40d6cd448fe5e82',
    'third_party/candle-core/src/quantized/cuda.rs':
        '7a021df686166f3c5a4af678eb09d076a6b9c5feee86613fce8eef703c76e089',
    'third_party/candle-core/src/quantized/k_quants.rs':
        '1ffd4d6fcc85449aa86b7a7e23623c6a6bcb35f5aa2140d4603b9a2dc071a371',
    'third_party/candle-core/src/quantized/mod.rs':
        '2c1d72dace7af3a8f549393818cf550b7916eb80cbb0a4a635f23b37204bb28b',
    'third_party/candle-core/src/quantized/neon.rs':
        'f1ff6d73bec79d4ddef5320d8c49a70c1bdb839363bc58adfb45634a2c56afc0',
    'third_party/candle-core/src/tensor.rs':
        '25b95a8deecffa87d55325d9ae258b9552778a7eb24a653b7c1438340aa21487',
    'third_party/candle-core/tests/quantized_tests.rs':
        'b039387e3d28f7477e3477d2587b2bbe5f050478c8d3904447014dfcb47d9086',
    'third_party/candle-core/tests/tensor_tests.rs':
        '254b5b16c0e56d86e06f26494929263e04b30827017be4f70c51fd3304ebaccb',
}


def root_items(text):
    start = depth = 0
    for index, char in enumerate(text):
        if char == '{': depth += 1
        elif char == '}': depth -= 1
        elif char == ',' and depth == 0:
            yield text[start:index].strip()
            start = index + 1
    yield text[start:].strip()


class SourceLayout(unittest.TestCase):
    def test_root_contains_only_the_two_crate_entry_points(self):
        self.assertEqual({p.name for p in SOURCE.glob('*.rs')}, {'lib.rs', 'main.rs'})
        entry = (SOURCE / 'main.rs').read_text(encoding='utf-8')
        self.assertIn('maris::cli::main()', entry)
        self.assertLessEqual(len(entry.splitlines()), 12)
        declarations = set(re.findall(r'^pub mod (\w+);$', (SOURCE / 'lib.rs').read_text(encoding='utf-8'), re.M))
        self.assertEqual(declarations, DOMAINS)
        for domain in DOMAINS:
            self.assertTrue((SOURCE / domain / 'mod.rs').is_file(), domain)

    def test_all_production_files_have_one_module_owner(self):
        visited = set()
        def visit(file):
            file = file.resolve()
            self.assertNotIn(file, visited, f'Duplicate module ownership: {file}')
            visited.add(file)
            text = file.read_text(encoding='utf-8')
            base = file.parent if file.name in ('lib.rs', 'main.rs', 'mod.rs') else file.parent / file.stem
            for match in MODULE.finditer(text):
                attrs, name = match[1], match[3]
                explicit = re.search(r'#\[path\s*=\s*"([^"]+)"\]', attrs)
                if explicit:
                    target = (file.parent / explicit[1]).resolve()
                    self.assertTrue(target.is_file(), f'Missing included module: {target}')
                    if target.is_relative_to(ROOT / 'tests'):
                        self.assertIn('#[cfg(test)]', attrs, str(file))
                        continue
                    self.assertTrue(target.is_relative_to(SOURCE), str(target))
                    # The retained opt-in legacy macOS route adapter is the sole source-path alias.
                    self.assertEqual(file.relative_to(ROOT).as_posix(), 'src/audio/routing.rs')
                else:
                    candidates = [base / (name + '.rs'), base / name / 'mod.rs']
                    found = [p for p in candidates if p.is_file()]
                    self.assertEqual(len(found), 1, f'Missing or ambiguous module {name} in {file}')
                    target = found[0]
                visit(target)
        visit(SOURCE / 'lib.rs')
        visit(SOURCE / 'main.rs')
        self.assertEqual(visited, {p.resolve() for p in SOURCE.rglob('*.rs')})

    def test_internal_imports_do_not_depend_on_legacy_root_exports(self):
        library = (SOURCE / 'lib.rs').read_text(encoding='utf-8')
        aliases = set()
        for path, alias in re.findall(r'^pub use ([\w:]+)(?: as (\w+))?;', library, re.M):
            aliases.add(alias or path.split('::')[-1])
        for file in SOURCE.rglob('*.rs'):
            if file.name in ('lib.rs', 'main.rs') and file.parent == SOURCE:
                continue
            text = file.read_text(encoding='utf-8')
            for alias in aliases:
                self.assertIsNone(re.search(r'\bcrate::' + re.escape(alias) + r'\b', text), f'Legacy import in {file}: {alias}')
            for body in re.findall(r'\buse\s+crate::\{(.*?)\};', text, re.S):
                for item in root_items(body):
                    name = re.match(r'(\w+)', item)
                    if name:
                        self.assertNotIn(name[1], aliases, f'Legacy grouped import in {file}')

    def test_dsp_and_matrix_math_do_not_import_presentation_or_transport(self):
        files = list((SOURCE / 'dsp').glob('*.rs')) + [SOURCE / 'mixer/engine.rs']
        forbidden = r'\b(?:crate::(?:ui|cli)|std::(?:fs|net|process)|ureq|cpal|ratatui|crossterm)::'
        for file in files:
            self.assertIsNone(re.search(forbidden, file.read_text(encoding='utf-8')), str(file))

    def test_maintained_source_files_stay_below_the_project_line_limit(self):
        # Tests and locally maintained vendored Rust keep the same line limit.
        roots = (SOURCE, ROOT / 'tests', ROOT / 'third_party')
        files = [file for root in roots for file in root.rglob('*.rs')]
        self.assertTrue(files)
        observed_baselines = set()
        for file in files:
            relative = file.relative_to(ROOT).as_posix()
            contents = file.read_bytes()
            if relative in UPSTREAM_LINE_BASELINES:
                self.assertEqual(hashlib.sha256(contents).hexdigest(),
                                 UPSTREAM_LINE_BASELINES[relative], relative)
                observed_baselines.add(relative)
            else:
                self.assertLessEqual(len(contents.decode('utf-8').splitlines()), 1000, str(file))
            if file.is_relative_to(SOURCE):
                self.assertLessEqual(len(file.stem), 24, str(file))
        self.assertEqual(observed_baselines, set(UPSTREAM_LINE_BASELINES))

    def test_contributor_rules_and_architecture_describe_the_actual_modules(self):
        guide = (ROOT / 'AGENTS.md').read_text(encoding='utf-8')
        architecture = (ROOT / 'docs/reference/architecture.md').read_text(encoding='utf-8')
        design = (ROOT / '.wcode/design/product.yaml').read_text(encoding='utf-8')
        self.assertIn('src/main.rs', guide)
        self.assertIn('src/lib.rs', guide)
        for path in ('ui/tui/settings', 'ui/tui/studio', 'ui/desktop', 'dsp', 'devices', 'tuning'):
            self.assertIn(path, architecture)
            self.assertIn('src/' + path, design)


if __name__ == '__main__':
    unittest.main()
