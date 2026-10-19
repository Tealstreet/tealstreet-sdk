#!/usr/bin/env python3
"""Require compiled handler-registry faults and byte-exact restored binding passes."""
import argparse
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--logs', type=Path, required=True)
args = parser.parse_args()
workspace = Path(__file__).resolve().parents[2]
args.logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
faults = [
    ('statistics', workspace / 'safe-cex-exchange/src/js_runtime/sockets.rs',
     'read_handler_stats(&core, topic, missing).map_err(core_error)',
     'read_handler_stats(&core, topic, missing).map(|(_, count)| (false, count)).map_err(core_error)',
     ['--test', 'exchange_binding'],
     'recorded_business_candle_tracks_registered_handlers_and_unsubscribe',
     'recorded candle has the source handler registration'),
    ('selected-key', workspace / 'safe-cex-venue-okx/src/binding/sockets.rs',
     'let callbacks = this\n        .get("messageHandlers")?\n        .get_key(&handler.to_js_string()?)?;',
     'let table = this.get("messageHandlers")?; table.own_entries()?; let callbacks = table.get_key(&handler.to_js_string()?)?;',
     ['--lib'], 'actual_socket_dispatch_reads_only_the_live_selected_handler',
     'unrelated handler getter must stay unread'),
    ('live-array', workspace / 'safe-cex-venue-okx/src/binding/sockets.rs',
     'let iterator = IteratorRecord::get(&callbacks)?;',
     'let iterator = IteratorRecord::get(&Js::array(callbacks.iterable_values()?))?;',
     ['--lib'], 'actual_dispatch_keeps_captured_live_arrays_and_original_failures',
     'captures the array and iterates its live length'),
]

for label, source, needle, fault, target, name, failure in faults:
    original = source.read_bytes()
    assert original.count(needle.encode()) == 1, label
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p',
               'safe-cex-venue-okx', '--all-features', *target, name, '--', '--nocapture']

    def run(phase):
        result = subprocess.run(command, cwd=workspace, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = result.stdout.decode(errors='replace')
        (args.logs / f'{label}-{phase}.log').write_text(output)
        if phase == 'red':
            assert result.returncode == 101 and '1 failed' in output, output[-8000:]
            assert name + ' ... FAILED' in output and failure in output, output[-8000:]
            assert 'could not compile' not in output, output[-8000:]
        else:
            assert result.returncode == 0 and '1 passed; 0 failed' in output, output[-8000:]
        print(f'{label}: compiled {phase.upper()}', flush=True)

    try:
        source.write_bytes(original.replace(needle.encode(), fault.encode()))
        run('red')
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run('green')
