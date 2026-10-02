#!/usr/bin/env python3
"""Run source-bound offline rounds; optional bounded batches resume only verified evidence."""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
from source_audit import ROOT, source_digest

SUITES = ["config_editor", "config_layout", "config_interaction", "config_safety",
          "menu_configuration", "control_messages", "preference_safety", "audio_quality",
          "scenes", "installer", "click_delivery", "feature_ablation"]


def test_passed(data: bytes, returncode: int | None) -> bool:
    results = re.findall(rb"test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored;", data)
    return returncode == 0 and len(results) == 1 and int(results[0][0]) > 0 and results[0][1:] == (b"0", b"0")


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open('rb') as file:
        for block in iter(lambda: file.read(1024 * 1024), b''):
            hasher.update(block)
    return hasher.hexdigest()


def read_owned(path: Path, bound: int) -> bytes:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > bound:
        raise ValueError('Missing, linked or oversized verification evidence')
    return path.read_bytes()


def checkpoint_binaries(binaries: dict[str, str], root: Path = ROOT) -> dict:
    result = {}
    if (root / 'target').is_symlink():
        raise ValueError('Linked compiled target directory rejected')
    for name, value in binaries.items():
        path = Path(value)
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to((root / 'target').resolve()):
            raise ValueError('Regression binary must be an actual compiled target')
        result[name] = {'path': str(path.resolve()), 'sha256': digest(path)}
    if set(result) != set(SUITES + ['release_matrix']):
        raise ValueError('Incomplete regression binary set')
    return result


def resume_records(output: Path, checkpoint: dict, revision: str, root: Path = ROOT) -> tuple[dict[str, str], list[dict]]:
    if checkpoint.get('schema_version') != 1 or checkpoint.get('source_sha256') != revision:
        raise ValueError('Source changed; this verification run cannot resume')
    rounds = checkpoint.get('requested_rounds')
    if type(rounds) is not int or not 1 <= rounds <= 10000:
        raise ValueError('Invalid original round count')
    stored = checkpoint.get('binaries', {})
    binaries = {name: item['path'] for name, item in stored.items()}
    if checkpoint_binaries(binaries, root) != stored:
        raise ValueError('Compiled binaries changed; start a new verification run')
    path = output / 'rounds.jsonl'
    records = [json.loads(line) for line in read_owned(path, 16 * 1024 * 1024).splitlines()]
    if len(records) > rounds:
        raise ValueError('Too many existing verification rounds')
    for index, record in enumerate(records, 1):
        expected = ['release_matrix', SUITES[(index - 1) % len(SUITES)]]
        steps = record.get('steps', [])
        if (type(record.get('round')) is not int or type(record.get('seed')) is not int
                or record.get('round') != index or record.get('seed') != index or record.get('passed') is not True
                or len(steps) != 2 or [step.get('suite') for step in steps] != expected):
            raise ValueError('Failed, duplicate, missing or out-of-order evidence cannot resume')
        for step in steps:
            name = f"round-{index:03d}-{step['suite']}.log"
            if step.get('log') != name or step.get('passed') is not True:
                raise ValueError('Invalid retained verification step')
            data = read_owned(output / name, 32 * 1024 * 1024)
            if hashlib.sha256(data).hexdigest() != step.get('log_sha256') or not test_passed(data, step.get('exit_code')):
                raise ValueError('Retained verification log was modified or did not pass')
    return binaries, records


def run_round(index: int, binaries: dict[str, str], output: Path) -> dict:
    """Each test process uses its own temporary state; no shared live audio or user profile."""
    suite = SUITES[(index - 1) % len(SUITES)]
    env = dict(os.environ, MARIS_CHECK_SEED=str(index))
    start = time.monotonic()
    steps = []
    for name in ['release_matrix', suite]:
        log_path = output / f'round-{index:03d}-{name}.log'
        if log_path.is_symlink():
            raise ValueError('Linked round log rejected')
        try:
            result = subprocess.run([binaries[name], '--nocapture'], cwd=ROOT, env=env,
                                    capture_output=True, timeout=120)
            data = result.stdout + b'\n' + result.stderr
            passed, code = test_passed(data, result.returncode), result.returncode
        except subprocess.TimeoutExpired as error:
            data = (error.stdout or b'') + (error.stderr or b'') + b'\nTIMED OUT'
            passed, code = False, None
        log_path.write_bytes(data)
        steps.append({'suite': name, 'exit_code': code, 'passed': passed,
                      'log': log_path.name, 'log_sha256': hashlib.sha256(data).hexdigest()})
        if not passed:
            break
    return {'round': index, 'seed': index, 'passed': all(s['passed'] for s in steps),
            'duration_seconds': round(time.monotonic() - start, 3), 'steps': steps}


def compile_binaries(output: Path) -> dict[str, str]:
    command = ['cargo', 'test', '--locked', '--no-run', '--message-format=json']
    for suite in ['release_matrix'] + SUITES:
        command += ['--test', suite]
    build = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=600)
    (output / 'compile.log').write_text(build.stderr, encoding='utf-8')
    if build.returncode:
        raise RuntimeError('Test compilation failed; see ' + str(output.relative_to(ROOT)))
    binaries = {}
    for line in build.stdout.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get('reason') == 'compiler-artifact' and item.get('executable'):
            name = item['target']['name']
            if name in SUITES + ['release_matrix']:
                binaries[name] = item['executable']
    checkpoint_binaries(binaries)
    return binaries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rounds', type=int, help='Total distinct seeds; default 200 on a new run')
    parser.add_argument('--jobs', type=int, choices=range(1, 9), default=4,
                        help='Independent test processes; default 4, maximum 8')
    parser.add_argument('--batch-size', type=int, help='At most this many new rounds; partial reports never pass')
    parser.add_argument('--resume', type=Path, help='Resume one existing source/binary/log-verified run directory')
    args = parser.parse_args()
    if (args.rounds is not None and not 1 <= args.rounds <= 10000
            or args.batch_size is not None and not 1 <= args.batch_size <= 10000):
        parser.error('Round counts and batch size must be between 1 and 10000')
    revision = source_digest()
    root = ROOT / '.maris-review/verification'
    if root.is_symlink() or root.parent.is_symlink():
        raise ValueError('Linked verification directory rejected')
    root.mkdir(parents=True, exist_ok=True)
    if args.resume:
        output = (ROOT / args.resume).absolute()
        if output.is_symlink() or output.parent.resolve() != root.resolve() or not output.is_dir():
            raise ValueError('Resume must name one existing owned verification directory')
    else:
        output = root / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
        output.mkdir()
    lock = output / '.batch.lock'
    # An interrupted live batch is never unlocked automatically. Inspect it before recovery.
    with lock.open('x', encoding='ascii') as file:
        file.write(str(os.getpid()))
    try:
        if args.resume:
            checkpoint = json.loads(read_owned(output / 'run.json', 65536))
            binaries, records = resume_records(output, checkpoint, revision)
            rounds = checkpoint['requested_rounds']
            if args.rounds is not None and args.rounds != rounds:
                raise ValueError('Resume cannot change the original round count')
        else:
            rounds = args.rounds or 200
            binaries = compile_binaries(output)
            if source_digest() != revision:
                raise ValueError('Source changed while compiling; verification not started')
            checkpoint = {'schema_version': 1, 'source_sha256': revision,
                          'requested_rounds': rounds, 'binaries': checkpoint_binaries(binaries)}
            (output / 'run.json').write_text(json.dumps(checkpoint, indent=2) + '\n', encoding='utf-8')
            (output / 'rounds.jsonl').touch(exist_ok=False)
            records = []
        stop_reason = None
        end = min(rounds, len(records) + args.batch_size) if args.batch_size else rounds
        with ThreadPoolExecutor(max_workers=args.jobs) as workers:
            for first in range(len(records) + 1, end + 1, args.jobs):
                if source_digest() != revision:
                    stop_reason = 'Source changed during verification; evidence invalidated'
                    break
                futures = [workers.submit(run_round, index, binaries, output)
                           for index in range(first, min(first + args.jobs, end + 1))]
                for future in futures:
                    record = future.result(); records.append(record)
                    with (output / 'rounds.jsonl').open('a', encoding='utf-8') as file:
                        file.write(json.dumps(record) + '\n')
                    if record['round'] % 20 == 0 or not record['passed']:
                        print(f"Round {record['round']}/{rounds}: {'PASS' if record['passed'] else 'FAIL'}", flush=True)
                    if not record['passed'] and stop_reason is None:
                        stop_reason = f"Round {record['round']} failed; no automatic retry or failure suppression"
                if stop_reason:
                    break
        stable = source_digest() == revision
        healthy = stable and stop_reason is None and all(r['passed'] for r in records)
        complete = len(records) == rounds
        report = {'schema_version': 1, 'source_sha256': revision, 'source_unchanged': stable,
                  'requested_rounds': rounds, 'completed_rounds': len(records),
                  'passed_rounds': sum(r['passed'] for r in records), 'passed': healthy and complete,
                  'complete': complete, 'stop_reason': stop_reason, 'platform': os.sys.platform,
                  'workers': args.jobs, 'independent_audits': False, 'full_suite_repetitions': False,
                  'hardware_validation': False, 'subjective_listening_validation': False,
                  'method': 'one seeded real DSP/draft case and one rotating regression suite per round',
                  'records': records}
        # Only a complete run passes; a successful bounded batch is explicitly unfinished.
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'passed': report['passed'], 'complete': complete, 'batch_ok': healthy,
                          'rounds': len(records), 'report': str((output / 'report.json').relative_to(ROOT))}))
        return 0 if healthy and (complete or args.batch_size is not None) else 1
    finally:
        lock.unlink()


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError, KeyError, TypeError) as error:
        raise SystemExit(str(error)) from error
