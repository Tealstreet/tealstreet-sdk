#!/usr/bin/env python3
"""Prove the constructed native socket test detects retained dispatch packets."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/public_classes.rs'
original = source.read_bytes()
before = b'        self.0.borrow_mut().pop();'
assert original.count(before) == 1
logs = crate.parent / 'target/public-frame-guard-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'public_frame_guard::production_native_public_frame_guard_releases_success_error_and_unwind'
command = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
           '-p', 'safe-cex-venue-binance', '--features', 'native', '--test', 'private_classes',
           name, '--', '--exact']


def execute(label):
    path = logs / (label + '.log')
    with path.open('w') as output:
        result = subprocess.run(command, cwd=crate.parent,
                                env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


try:
    source.write_bytes(original.replace(before, b'        let _frames = self.0.borrow();'))
    status, log = execute('retained-packet-red')
    assert status != 0 and f'test {name} ... FAILED' in log
    assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M)
    assert 'native frame guard releases parsed packet after outcome 0' in log
finally:
    source.write_bytes(original)
    assert source.read_bytes() == original
status, log = execute('exact-restoration-green')
assert status == 0 and '1 passed; 0 failed' in log
print('Compiled packet-retention assertion RED; exact source restoration GREEN.', flush=True)
