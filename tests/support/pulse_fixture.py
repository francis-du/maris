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
if args and args[0] in ('--format=json', '--format=text'):
    args = args[1:]
if args == ['info']:
    print(json.dumps({'default_sink_name': state['default']}))
elif args == ['list', 'short', 'modules']:
    for module in state['modules']:
        print(str(module['index']) + '\t' + module['name'] + '\t' + module['argument'] + '\t')
elif len(args) == 2 and args[0] == 'list':
    key = {'sinks': 'sinks', 'sink-inputs': 'inputs', 'modules': 'modules'}[args[1]]
    items = state[key]
    # Match independent native pactl 16.1 output instead of mirroring Rust structs.
    if key == 'modules':
        items = [{k: v for k, v in item.items() if k != 'index'} for item in items]
    elif key == 'sinks':
        items = [{('monitor_source' if k == 'monitor_source_name' else k): v
                  for k, v in item.items()} for item in items]
    elif key == 'inputs':
        items = [dict(item, client=str(item['client']) if item.get('client') is not None else None)
                 for item in items]
    print(json.dumps(items))
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
