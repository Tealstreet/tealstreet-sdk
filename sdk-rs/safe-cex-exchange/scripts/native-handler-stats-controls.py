#!/usr/bin/env python3
"""Compile a missing-registry-reader fault, require each binding failure, restore."""
import argparse
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--logs', type=Path, required=True)
args = parser.parse_args()
workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/websocket.rs'
args.logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL='0')
original = source.read_bytes()
needle = b'if let Some(count) = count(topic)? {'
assert original.count(needle) == 1
fault = b'if let Some(count) = count(topic)?.filter(|_| false) {'
cases = [
    ('apexomni', ['--test', 'binding'], 'recorded_public_pong_uses_registered_native_handlers'),
    ('bingx', ['--test', 'binding'], 'bound_listen_ohlcv'),
    ('grvt', ['--test', 'exchange_binding'], 'socket_candles_keep_source_aliases_time_units_and_cleanup'),
    ('coinbase', ['--test', 'exchange_binding'], 'public_stream_listeners_use_real_parser_and_shared_book_operations'),
    ('backpack', ['--features', 'native', '--test', 'adapter_classes'], 'native_market_frames_reach_sdk_tickers_books_and_candles'),
]

def run(phase):
    for venue, target, name in cases:
        command = ['cargo', 'test', '--locked', '--jobs', '2', '-p',
                   'safe-cex-venue-' + venue, *target, name, '--', '--nocapture']
        print(f'{venue}: compiling {phase} binding control', flush=True)
        result = subprocess.run(command, cwd=workspace, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = result.stdout.decode(errors='replace')
        (args.logs / f'{venue}-{phase}.log').write_text(output)
        if phase == 'red':
            assert result.returncode == 101 and f'{name} ... FAILED' in output, output[-8000:]
            assert 'could not compile' not in output, output[-8000:]
            assert 'left: Some(false)' in output and 'right: Some(true)' in output, output[-8000:]
        else:
            assert result.returncode == 0 and '1 passed; 0 failed' in output, output[-8000:]
        print(f'{venue}: compiled {phase.upper()}', flush=True)

try:
    source.write_bytes(original.replace(needle, fault))
    run('red')
finally:
    source.write_bytes(original)
assert source.read_bytes() == original
run('green')
