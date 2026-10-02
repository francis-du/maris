#!/usr/bin/env python3
"""Isolated protocol fixture. No audio server, device, network or host settings are accessed."""
import json
from pathlib import Path
import sys

address, *args = sys.argv[1:]
if not address.startswith('--server=unix:'):
    raise SystemExit(2)
root = Path(address.removeprefix('--server=unix:')).parent
path = root / 'fixture.json'
state = json.loads(path.read_text())
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps(args) + '\n')
if args and args[0] == '--format=json':
    args = args[1:]
if args == ['info']:
    print(json.dumps({'default_sink_name': state['default']}))
elif len(args) == 2 and args[0] == 'list':
    key = {'sinks': 'sinks', 'sink-inputs': 'inputs', 'modules': 'modules'}[args[1]]
    print(json.dumps(state[key]))
elif len(args) == 3 and args[0] == 'move-sink-input':
    if state.pop('fail_move', False):
        path.write_text(json.dumps(state))
        raise SystemExit(1)
    target = next(s for s in state['sinks'] if s['name'] == args[2])
    source = next(s for s in state['inputs'] if s['index'] == int(args[1]))
    if target['name'].startswith('maris_'):
        records = list((root / 'state').glob('pulse-route-*.json')) + [root / 'state/pulse-route.json']
        journals = [json.loads(record.read_text()) for record in records if record.is_file()]
        journal = next((item for item in journals if item.get('sink') == target['name']), None)
        if journal is None or not any(r['input']['index'] == source['index'] for r in journal['routes']):
            raise SystemExit('Route was changed before its recovery record was written')
    source['sink'] = target['index']
    path.write_text(json.dumps(state))
elif len(args) == 2 and args[0] == 'unload-module':
    module = int(args[1])
    state['modules'] = [m for m in state['modules'] if m['index'] != module]
    state['sinks'] = [s for s in state['sinks'] if s.get('owner_module') != module]
    path.write_text(json.dumps(state))
else:
    raise SystemExit('Unexpected fixture operation: ' + repr(args))
