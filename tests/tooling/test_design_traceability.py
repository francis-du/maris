"""Keep Design State references resolvable to declared entries and real files.

The design files use a regular block shape, so the gate reads identifiers and
paths directly instead of adding a YAML dependency to the tooling environment.
It fails on a dangling requirement→component, requirement→acceptance,
requirement→constraint or component→dependency reference, and on an implementation
path that no longer exists, rather than letting the mapping rot silently.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
DESIGN = ROOT / '.wcode' / 'design'
ID = re.compile(r'(?m)^\s+id:\s*(\S+)\s*$')
PATH = re.compile(r'(?m)^\s+path:\s*(\S+)\s*$')


def blocks(name):
    text = (DESIGN / name).read_text(encoding='utf-8')
    return [block for block in re.split(r'\n(?=- schema_version:)', text) if ID.search(block)]


def declared_ids(name):
    return {match.group(1) for block in blocks(name) for match in ID.finditer(block)}


def listed_values(block, key):
    """Return a list key's values in either inline `[a, b]` or block `- a` form."""
    match = re.search(r'(?m)^\s*' + key + r':\s*\[([^\]]*)\]\s*$', block)
    if match:
        return [value.strip() for value in match.group(1).split(',') if value.strip()]
    match = re.search(
        r'(?m)^\s*' + key + r':\s*\n((?:^[ \t]+-[ \t]*\S+[ \t]*\n?)+)', block, re.MULTILINE
    )
    if match:
        return re.findall(r'^[ \t]+-[ \t]*(\S+)', match.group(1), re.MULTILINE)
    return []


def referenced(name, key):
    return [(block_id.group(1), value)
            for block in blocks(name)
            for block_id in [ID.search(block)]
            for value in listed_values(block, key)]


def implementation_paths(name):
    return [match.group(1) for block in blocks(name) for match in PATH.finditer(block)]


class DesignTraceability(unittest.TestCase):
    def test_requirements_reference_declared_entries(self):
        expected = {
            'implemented_by': declared_ids('components.yaml'),
            'acceptance': declared_ids('acceptance.yaml'),
            'constraints': declared_ids('constraints.yaml'),
        }
        dangling = []
        for key, known in expected.items():
            for requirement, value in referenced('requirements.yaml', key):
                if value not in known:
                    dangling.append(f'{requirement} -> {key} {value}')
        self.assertEqual(dangling, [], 'requirement references with no declared target')

    def test_component_dependencies_reference_declared_components(self):
        known = declared_ids('components.yaml')
        dangling = [f'{component} -> {value}'
                    for component, value in referenced('components.yaml', 'depends_on')
                    if value not in known]
        self.assertEqual(dangling, [], 'component dependencies with no declared target')

    def test_component_implementation_paths_exist(self):
        missing = [path for path in implementation_paths('components.yaml') if not (ROOT / path).is_file()]
        self.assertEqual(missing, [], 'component implementation paths that do not exist')

    def test_acceptance_verification_paths_exist(self):
        missing = [path for path in implementation_paths('acceptance.yaml') if not (ROOT / path).is_file()]
        self.assertEqual(missing, [], 'acceptance verification paths that do not exist')

    def test_every_requirement_declares_at_least_one_component(self):
        unmapped = []
        for block in blocks('requirements.yaml'):
            requirement = ID.search(block).group(1)
            if not listed_values(block, 'implemented_by'):
                unmapped.append(requirement)
        self.assertEqual(unmapped, [], 'requirements with no component mapping')


if __name__ == '__main__':
    unittest.main()
