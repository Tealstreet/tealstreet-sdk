#!/usr/bin/env python3
"""Run serially, without another Cargo invocation; restore exact production bytes."""
import hashlib
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
core = workspace / 'safe-cex-exchange/src'
logs = workspace / 'target/bybit-eager-dial-controls'
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL='0')
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(workspace.parent / 'node_modules/typescript'))

def run(label, package, target, name, assertion, red):
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', package, '--all-features']
    command += ['--lib'] if target == 'lib' else ['--test', target]
    command += [name, '--', '--exact']
    result = subprocess.run(command, cwd=workspace, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (logs / f'{label}.log').write_text(output)
    assert 'error[E' not in output and 'could not compile' not in output, (label, 'must compile')
    assert 'running 1 test' in output, (label, 'named witness must execute')
    if red:
        assert result.returncode == 101 and assertion in output, (label, 'named production assertion must fail')
    else:
        assert result.returncode == 0, (label, output[-2000:])
    print(label, 'RED' if red else 'GREEN', flush=True)

witness = ('safe-cex-venue-bybit', 'lib', 'ws_public::tests::dial_tests::disposing_before_dial_is_polled_releases_venue_socket')
cases = [
    ('lazy-prefix', core / 'websocket.rs', '            false,\n            true,\n        );', '            false,\n            false,\n        );', [(*witness, 'BYBIT-EAGER-DIAL before any host pump')]),
    ('cancelled-io-leak', core / 'owned_tasks.rs', '        drop(future);\n        drop(host);', '        std::mem::forget(future);\n        drop(host);', [(*witness, 'BYBIT-DIAL-CANCEL actual I/O drops before host pump')]),
]
for label, source, before, after, witnesses in cases:
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (label, 'exact production anchor')
    digest = hashlib.sha256(original).digest()
    try:
        source.write_text(text.replace(before, after, 1))
        for index, witness in enumerate(witnesses):
            run(f'{label}-{index}-red', *witness, red=True)
    finally:
        source.write_bytes(original)
        assert hashlib.sha256(source.read_bytes()).digest() == digest
        for index, witness in enumerate(witnesses):
            run(f'{label}-{index}-restored', *witness, red=False)
print('ALL EAGER DIAL CONTROLS COMPLETE', flush=True)
