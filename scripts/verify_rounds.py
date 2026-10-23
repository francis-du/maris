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
import stat
import subprocess
import time
from source_audit import ROOT, source_digest
from release_bundle import linked_directory

SUITES = ["config_editor", "config_layout", "config_interaction", "config_safety",
          "menu_configuration", "control_messages", "preference_safety", "audio_quality",
          "scenes", "installer", "click_delivery", "feature_ablation"]


def test_passed(data: bytes, returncode: int | None) -> bool:
    results = re.findall(rb"test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored;", data)
    return (type(returncode) is int and returncode == 0 and len(results) == 1
            and int(results[0][0]) > 0 and results[0][1:] == (b"0", b"0"))


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open('rb') as file:
        for block in iter(lambda: file.read(1024 * 1024), b''):
            hasher.update(block)
    return hasher.hexdigest()


def read_owned(path: Path, bound: int) -> bytes:
    """Read one bounded, unchanged regular file; path metadata alone is not a read limit."""
    if type(bound) is not int or bound < 0:
        raise ValueError('Invalid verification evidence bound')

    def identity(info: os.stat_result) -> tuple:
        # Windows path ctime can be birth time while fstat ctime is change time.
        # Compare each API's ctime with its own earlier snapshot below.
        return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
                info.st_mtime_ns, None if os.name == 'nt' else info.st_ctime_ns,
                getattr(info, 'st_file_attributes', 0))

    try:
        before = path.lstat()
        if (not stat.S_ISREG(before.st_mode) or linked_directory(path)
                or before.st_size > bound):
            raise ValueError('Missing, linked or oversized verification evidence')
        # NOFOLLOW rejects a final-component link swap. NONBLOCK prevents a FIFO
        # replacement from hanging before fstat can reject it. Windows still gets
        # descriptor/path identity checks where these flags are unavailable.
        flags = (os.O_RDONLY | getattr(os, 'O_BINARY', 0)
                 | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0))
        with os.fdopen(os.open(path, flags), 'rb') as file:
            opened = os.fstat(file.fileno())
            if identity(opened) != identity(before):
                raise ValueError('Verification evidence changed before reading')
            data = file.read(bound + 1)
            if len(data) > bound:
                raise ValueError('Oversized verification evidence')
            after = os.fstat(file.fileno())
            current = path.lstat()
            if (len(data) != before.st_size
                    or identity(after) != identity(opened)
                    or after.st_ctime_ns != opened.st_ctime_ns
                    or identity(current) != identity(before)
                    or current.st_ctime_ns != before.st_ctime_ns):
                raise ValueError('Verification evidence changed while reading')
        return data
    except OSError as error:
        raise ValueError('Missing, linked or unreadable verification evidence') from error


def checkpoint_binaries(binaries: dict[str, str], root: Path = ROOT) -> dict:
    result = {}
    target = root / 'target'
    if linked_directory(target) or not target.is_dir():
        raise ValueError('Linked or invalid compiled target directory rejected')
    target = target.resolve()
    for name, value in binaries.items():
        path = Path(value).absolute()
        current = path.parent
        while current != target and current != current.parent:
            if linked_directory(current) or not current.is_dir():
                raise ValueError('Regression binary parent must be a real compiled target directory')
            current = current.parent
        if linked_directory(path) or not path.is_file() or not path.resolve().is_relative_to(target):
            raise ValueError('Regression binary must be an actual compiled target')
        result[name] = {'path': str(path.resolve()), 'sha256': digest(path)}
    if set(result) != set(SUITES + ['release_matrix']):
        raise ValueError('Incomplete regression binary set')
    return result


def binary_state(binaries: dict[str, str]) -> dict:
    """Cheap per-batch drift guard; full hashes are still required at both run boundaries."""
    result = {}
    for name, value in binaries.items():
        path = Path(value)
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or linked_directory(path):
            raise ValueError('Regression binary is no longer a regular file')
        result[name] = (info.st_dev, info.st_ino, info.st_mode, info.st_size,
                        info.st_mtime_ns, info.st_ctime_ns, getattr(info, 'st_file_attributes', 0))
    return result


def resume_records(output: Path, checkpoint: dict, revision: str, root: Path = ROOT) -> tuple[dict[str, str], list[dict]]:
    invalidated = output / 'invalidated.json'
    if invalidated.exists() or invalidated.is_symlink():
        raise ValueError('Invalidated verification evidence cannot resume; start a new run')
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


def verification_root(root: Path = ROOT) -> Path:
    root = root.resolve()
    review = root / '.maris-review'
    verification = review / 'verification'
    for path in (review, verification):
        if linked_directory(path):
            raise ValueError('Linked verification directory rejected')
        if path.exists() and not path.is_dir():
            raise ValueError('Verification path contains a non-directory component')
    verification.mkdir(parents=True, exist_ok=True)
    return verification


def resume_directory(path: Path, root: Path) -> Path:
    path = path.absolute()
    if (linked_directory(path) or linked_directory(path.parent)
            or path.parent.resolve() != root.resolve() or not path.is_dir()):
        raise ValueError('Resume must name one existing owned verification directory')
    return path


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
    root = verification_root(ROOT)
    if args.resume:
        output = resume_directory(ROOT / args.resume, root)
    else:
        output = root / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
        output.mkdir()
    lock = output / '.batch.lock'
    # An interrupted live batch is never unlocked automatically. Inspect it before recovery.
    with lock.open('x', encoding='ascii') as file:
        file.write(str(os.getpid()))
    settled = False
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
        binary_baseline = binary_state(binaries)
        if (checkpoint_binaries(binaries) != checkpoint['binaries']
                or binary_state(binaries) != binary_baseline):
            raise ValueError('Compiled binaries changed before verification')
        stop_reason = None

        def binary_drift() -> str | None:
            try:
                if binary_state(binaries) == binary_baseline:
                    return None
            except (OSError, ValueError) as error:
                return f'Compiled binaries became unavailable during verification: {error}'
            return 'Compiled binaries changed during verification; evidence invalidated'

        end = min(rounds, len(records) + args.batch_size) if args.batch_size else rounds
        with ThreadPoolExecutor(max_workers=args.jobs) as workers:
            for first in range(len(records) + 1, end + 1, args.jobs):
                if source_digest() != revision:
                    stop_reason = 'Source changed during verification; evidence invalidated'
                    break
                stop_reason = binary_drift()
                if stop_reason:
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
                drift = binary_drift()
                if drift and stop_reason is None:
                    stop_reason = drift
                if stop_reason:
                    break
        stable = source_digest() == revision
        try:
            binaries_stable = (binary_drift() is None
                               and checkpoint_binaries(binaries) == checkpoint['binaries']
                               and binary_drift() is None)
        except (OSError, ValueError):
            binaries_stable = False
        if not binaries_stable and stop_reason is None:
            stop_reason = 'Compiled binaries changed during verification; evidence invalidated'
        healthy = stable and binaries_stable and stop_reason is None and all(r['passed'] for r in records)
        complete = len(records) == rounds
        if not healthy:
            # Restoring original binaries/source later must not turn invalid rounds
            # into reusable success. Retain this fail-closed marker across resumes.
            with (output / 'invalidated.json').open('x', encoding='utf-8') as file:
                json.dump({'reason': stop_reason or 'Source changed during verification'}, file)
                file.write('\n')
        report = {'schema_version': 1, 'source_sha256': revision, 'source_unchanged': stable,
                  'binaries_unchanged': binaries_stable,
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
        settled = True
        return 0 if healthy and (complete or args.batch_size is not None) else 1
    finally:
        # An exception can follow an executed test but precede its retained record
        # or final report. Keep the lock on every abnormal exit (including Ctrl-C),
        # preserving the cause and evidence instead of silently retrying that round.
        if settled:
            lock.unlink()


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError, KeyError, TypeError) as error:
        raise SystemExit(str(error)) from error
