#!/usr/bin/env python3
"""Run Maris against a private native PulseAudio server with two silent test outputs.

This is a native protocol/PCM integration test, not hardware or listening acceptance.
No host sound-server socket, driver, microphone or system audio configuration is used.
"""
from __future__ import annotations
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]


def wait_for(test, message: str, seconds: float = 15):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        try:
            value = test()
            if value:
                return value
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
            last = error
        time.sleep(0.05)
    raise RuntimeError(message + (f': {last}' if last else ''))


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding='utf-8'))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/maris')
    args = parser.parse_args()
    if sys.platform != 'linux' or os.getuid() == 0:
        raise SystemExit('This native integration test requires Linux and a non-root user.')
    tools = {name: shutil.which(name) for name in ('pulseaudio', 'pactl', 'pacat')}
    if not all(tools.values()):
        raise SystemExit('Native integration requires pulseaudio and pulseaudio-utils; nothing was started.')
    binary = args.binary.resolve(strict=True)
    binary.relative_to(ROOT)
    review = ROOT / '.maris-review'
    if review.is_symlink():
        raise SystemExit('Linked test output directory rejected.')
    review.mkdir(exist_ok=True)
    log_path = review / 'pulse-native.log'
    report = review / 'pulse-native.json'
    report.write_text(json.dumps({'passed': False, 'complete': False,
                                  'hardware_validation': False, 'listening_validation': False}) + '\n')
    with tempfile.TemporaryDirectory(prefix='maris-pulse-') as directory, log_path.open('wb') as log:
        root = Path(directory)
        home = root / 'home'; home.mkdir(mode=0o700)
        runtime = root / 'runtime'; runtime.mkdir(mode=0o700)
        (runtime / 'pulse').mkdir(mode=0o700)
        config = root / 'config/pulse'; config.mkdir(parents=True)
        (config / 'daemon.conf').write_text('flat-volumes = no\nexit-idle-time = -1\ndefault-sample-rate = 48000\nalternate-sample-rate = 48000\n')
        socket = runtime / 'pulse/native'
        address = 'unix:' + str(socket)
        state = root / 'state'
        env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime),
                   XDG_CONFIG_HOME=str(root / 'config'), MARIS_STATE_DIR=str(state),
                   PULSE_SERVER=address, LC_ALL='C')
        # These are silent server nodes, deliberately labeled as test ALSA endpoints so
        # the production endpoint selection is exercised without attaching sound hardware.
        server_script = root / 'server.pa'
        server_script.write_text(
            f'load-module module-native-protocol-unix socket={socket} auth-anonymous=1\n'
            'load-module module-null-sink sink_name=speakers_a rate=48000 channels=2 sink_properties="device.description=Fixture_Speakers_A device.api=alsa device.class=sound"\n'
            'load-module module-null-sink sink_name=speakers_b rate=48000 channels=2 sink_properties="device.description=Fixture_Speakers_B device.api=alsa device.class=sound"\n'
            'set-default-sink speakers_a\n')
        processes = []
        writers = []
        stop = threading.Event()

        def spawn(command, **kwargs):
            child = subprocess.Popen(command, env=env, stdout=log, stderr=log, **kwargs)
            processes.append(child)
            return child

        def pactl(*arguments):
            result = subprocess.run([tools['pactl'], '--server=' + address, '--format=json', *arguments],
                                    env=env, check=True, capture_output=True, text=True, timeout=5)
            return json.loads(result.stdout) if result.stdout.strip() else None

        def runtime_state():
            return read_json(state / 'runtime.json')

        def player(name: str, frequency: int):
            child = spawn([tools['pacat'], '--server=' + address, '--playback', '--raw',
                '--format=float32le', '--rate=48000', '--channels=2', '--device=speakers_a',
                '--property=application.id=audio.maris.test.player', '--stream-name=' + name], stdin=subprocess.PIPE)
            def feed():
                block = b''.join(struct.pack('<ff', *(2 * [0.05 * math.sin(i * math.tau * frequency / 48000)])) for i in range(4800))
                try:
                    while not stop.is_set():
                        child.stdin.write(block)
                        child.stdin.flush()
                except (BrokenPipeError, OSError, ValueError):
                    pass
            thread = threading.Thread(target=feed, daemon=True)
            writers.append(thread); thread.start()
            return child

        def apply_route(action: str, **values):
            current = runtime_state()
            command = dict(session_id=current['session_id'], action=action,
                           expected_rebind_count=current['rebind_count'], **values)
            temporary = state / 'smoke-command.tmp'
            temporary.write_text(json.dumps(command))
            os.replace(temporary, state / 'control.json')
            result = wait_for(lambda: runtime_state() if not (state / 'control.json').exists() else None,
                              'Control command was not consumed')
            wait_for(lambda: runtime_state().get('rebind_count', 0) > current['rebind_count'],
                     'Control did not apply: ' + action)
            return result

        try:
            daemon = spawn([tools['pulseaudio'], '--daemonize=no', '--use-pid-file=no', '--disable-shm=yes',
                            '--exit-idle-time=-1', '-nF', str(server_script)], stdin=subprocess.DEVNULL)
            wait_for(lambda: socket.is_socket() and pactl('info'), 'Private sound server did not start')
            sinks = pactl('list', 'sinks')
            if {sink['name'] for sink in sinks} != {'speakers_a', 'speakers_b'}:
                raise RuntimeError('The test server contains unexpected outputs.')
            originals = {sink['name']: (sink['volume'], sink['mute']) for sink in sinks}
            first = player('first_fixture_player', 440)
            second = player('second_fixture_player', 880)
            wait_for(lambda: len(pactl('list', 'sink-inputs')) == 2, 'Test players did not become ready')
            maris = spawn([str(binary), '--no-tray', 'system', '--accept-routing'], stdin=subprocess.DEVNULL)
            current = wait_for(lambda: (s if s.get('active') and s.get('system_backend') == 'pulse_server'
                                       and s.get('captured_frames', 0) > 4800 and s.get('frames', 0) > 4800 else None)
                               if (s := runtime_state()) else None, 'Maris did not process native PCM')
            if current['performance']['worker_frames'] == 0 or current['performance']['callback_calls'] != 0:
                raise RuntimeError('Native worker telemetry is missing or mislabeled as callback work.')
            private = next(sink for sink in pactl('list', 'sinks') if sink['name'].startswith('maris_'))
            inputs = pactl('list', 'sink-inputs')
            for entry in inputs:
                app = entry['properties'].get('application.id')
                if app == 'audio.maris.app' and entry['sink'] == private['index']:
                    raise RuntimeError('Maris captured its own output.')
                if app == 'audio.maris.test.player' and entry['sink'] != private['index']:
                    raise RuntimeError('An eligible application was not routed through DSP.')
            subprocess.run([str(binary), '--json', 'preamp', '-6'], env=env, check=True,
                           stdout=log, stderr=log, timeout=5)
            revision = read_json(state / 'profile.json')['revision']
            wait_for(lambda: runtime_state().get('applied_revision') == revision, 'DSP revision was not applied')
            apply_route('select_output', output='pulse:speakers_b')
            wait_for(lambda: runtime_state().get('device_identity', {}).get('stable_id') == 'speakers_b',
                     'New output binding was not applied')
            apply_route('select_applications', pids=[first.pid])
            wait_for(lambda: runtime_state().get('captured_application_pids') == [first.pid],
                     'Selected application scope was not applied')
            second_input = next(item for item in pactl('list', 'sink-inputs')
                                if str(item['properties'].get('application.process.id')) == str(second.pid))
            original_index = next(item['index'] for item in sinks if item['name'] == 'speakers_a')
            if second_input['sink'] != original_index:
                raise RuntimeError('Removing an application from the scope did not restore its output.')
            # Recovery is journaled per owned sink, not only in the legacy filename.
            records = list(state.glob('pulse-route-*.json'))
            if not records:
                raise RuntimeError('No token-scoped recovery journal was written.')
            for record in records:
                journal = read_json(record)
                if journal['sink'] != 'maris_' + journal['token']:
                    raise RuntimeError('Recovery journal does not bind its owned sink.')
            # This process belongs to the test. Force exit tests the real pipe guardian.
            maris.kill(); maris.wait(timeout=5)
            wait_for(lambda: not list(state.glob('pulse-route-*.json'))
                     and not (state / 'pulse-route.json').exists()
                     and not any(s['name'].startswith('maris_') for s in pactl('list', 'sinks')),
                     'Parent-exit recovery did not remove the owned route')
            first_input = next(item for item in pactl('list', 'sink-inputs')
                               if str(item['properties'].get('application.process.id')) == str(first.pid))
            if first_input['sink'] != original_index:
                raise RuntimeError('Parent-exit recovery did not restore the original output.')
            if pactl('info')['default_sink_name'] != 'speakers_a':
                raise RuntimeError('Maris changed the server default output.')
            for sink in pactl('list', 'sinks'):
                if (sink['volume'], sink['mute']) != originals[sink['name']]:
                    raise RuntimeError('Maris changed a test output volume or mute state.')
            if daemon.poll() is not None or first.poll() is not None or second.poll() is not None:
                raise RuntimeError('Maris terminated an unrelated test-server/client process.')
            result = {'passed': True, 'complete': True, 'native_server': 'pulseaudio',
                      'output_type': 'disposable_null_sinks', 'real_pcm_processed': True,
                      'revision_applied': True, 'output_switch': True, 'selected_application_scope': True,
                      'parent_exit_recovery': True, 'host_audio_accessed': False,
                      'hardware_validation': False, 'listening_validation': False}
            report.write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result))
        finally:
            stop.set()
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try: process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.wait(timeout=3)
            for thread in writers:
                thread.join(timeout=2)


if __name__ == '__main__':
    main()
