"""Shared public-release scope: site presentation cannot silently reduce user requirements."""
from __future__ import annotations
import json
from pathlib import Path

REQUIRED = ('application-audio', 'brand-readme', 'context-tuning', 'cross-platform-installers', 'desktop-monitor',
            'device-autoeq', 'documentation-site', 'dsp-regression', 'feature-ablation',
            'mixer-data-plane', 'model-acquisition', 'output-harshness', 'power-transition-noise', 'semantic-inference',
            'tui-completion', 'verification-delivery', 'windows-linux')
MANIFEST = Path('docs/development/requirements.json')


def load(root: Path) -> list[dict]:
    path = root / MANIFEST
    if path.is_symlink() or not path.is_file():
        raise ValueError('Required product scope is missing or linked')
    data = json.loads(path.read_text(encoding='utf-8'))
    if data.get('schema_version') != 1 or data.get('policy') != 'all_requested_work_before_release':
        raise ValueError('Invalid public-release scope policy')
    items = data.get('requirements', [])
    ids = [item.get('id') for item in items]
    if len(ids) != len(REQUIRED) or set(ids) != set(REQUIRED):
        raise ValueError('Previously requested requirements cannot disappear or be duplicated')
    for item in items:
        if item.get('status') not in ('pending', 'in_progress', 'verified'):
            raise ValueError('Unsupported requirement status')
    return items


def blockers(root: Path) -> list[str]:
    return [item['id'] for item in load(root) if item['status'] != 'verified']


def require_complete(root: Path, approval: dict, validate_evidence) -> None:
    items = load(root)
    unfinished = [item['id'] for item in items if item['status'] != 'verified']
    if unfinished:
        raise ValueError('Unfinished user requirements block release: ' + ', '.join(unfinished))
    evidence = approval.get('requirements', {})
    if not isinstance(evidence, dict) or set(evidence) != set(REQUIRED):
        raise ValueError('Every requested capability needs an explicit current-source acceptance record')
    for key in REQUIRED:
        validate_evidence(key, evidence[key])
