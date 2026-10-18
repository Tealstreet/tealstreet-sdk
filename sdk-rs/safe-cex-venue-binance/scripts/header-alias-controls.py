#!/usr/bin/env python3
"""Check the live native header reader and retained rejection response."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/native_host.rs'
original = source.read_bytes()
logs = crate.parent / 'target/header-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'production_native_request_weight_callbacks_observe_success_and_rejection'
cases = [
    ('header-getter', b'.call(receiver.clone(), vec![string(name)])',
     b'.call(receiver.clone(), vec![string(format!("unrelated-{name}"))])',
     'native request-weight observer must run on HTTP status 200'),
    ('retained-failure', b'        response_from_error(error)\n',
     b'        let _retained_error = error;\n        Ok(None)\n',
     'native request-weight observer must run on HTTP status 429'),
]


def execute(label):
    path = logs / (label + '.log')
    command = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
               '-p', 'safe-cex-venue-binance', '--features', 'native', '--test', 'private_classes',
               name, '--', '--exact']
    with path.open('w') as output:
        result = subprocess.run(command, cwd=crate.parent,
                                env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


for label, before, after, assertion in cases:
    assert original.count(before) == 1, label
    try:
        source.write_bytes(original.replace(before, after))
        status, log = execute(label + '-red')
        assert status != 0 and f'test {name} ... FAILED' in log, log[-5000:]
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), log[-5000:]
        assert assertion in log, log[-5000:]
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(label + '-restored')
    assert status == 0 and '1 passed; 0 failed' in log, log[-5000:]
    print(f'{label}: compiled assertion RED; exact restoration GREEN.', flush=True)
