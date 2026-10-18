#!/usr/bin/env python3
"""Check completion through the actual native private settlement handles."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
logs = crate.parent / 'target/settlement-controls'
logs.mkdir(parents=True, exist_ok=True)
cases = [
    ('settlement-ready', 'src/class_jobs.rs',
     b'return Poll::Ready(result.clone());',
     b'return match result { Ok(_) => Poll::Pending, Err(_) => Poll::Ready(result.clone()) };',
     'private_classes', 'production_native_private_clones_share_settlement_and_captured_receiver'),
]


def execute(target, name, label):
    path = logs / (label + '.log')
    command = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
               '-p', 'safe-cex-venue-binance', '--features', 'native', '--test', target,
               name, '--', '--exact']
    with path.open('w') as output:
        result = subprocess.run(command, cwd=crate.parent,
                                env={**os.environ, 'CARGO_BUILD_JOBS': '1', 'RUSTUP_TOOLCHAIN': '1.99.0'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


for label, file, before, after, target, name in cases:
    source = crate / file
    original = source.read_bytes()
    assert original.count(before) == 1, label
    try:
        source.write_bytes(original.replace(before, after))
        status, log = execute(target, name, label + '-red')
        assert status != 0 and f'test {name} ... FAILED' in log, log[-5000:]
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), log[-5000:]
        assert 'fixture exceeded independent 16-poll bound' in log, log[-5000:]
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(target, name, label + '-restored')
    assert status == 0 and '1 passed; 0 failed' in log, log[-5000:]
    print(f'{label}: compiled assertion RED; exact restoration GREEN.', flush=True)
