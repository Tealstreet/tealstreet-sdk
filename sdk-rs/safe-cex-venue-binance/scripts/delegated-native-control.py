#!/usr/bin/env python3
"""The maintained history method must actually fetch symbol-scoped native fills."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-binance/src/exchange_classes.rs'
original = source.read_bytes()
before = b'Method::HistoricalPositions => self.imports.fold_closed_positions(arg(0), true)?.await,'
after = b'Method::HistoricalPositions => self.imports.fold_closed_positions(arg(0), false)?.await,'
assert original.count(before) == 1
name = 'delegated_native_routes::production_native_historical_positions_keeps_symbol_scoped_fold'
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--test', 'private_classes', name, '--', '--exact']
logs = Path(tempfile.mkdtemp(prefix='binance-delegated-native-control-'))


def run(label):
    result = subprocess.run(command, cwd=sdk, env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (label + '.log')).write_text(result.stdout)
    return result


try:
    green = run('baseline')
    assert green.returncode == 0 and f'test {name} ... ok' in green.stdout, green.stdout
    source.write_bytes(original.replace(before, after))
    try:
        red = run('wrong-scope-red')
        assert red.returncode != 0 and f'test {name} ... FAILED' in red.stdout, red.stdout
        assert 'symbol-scoped fold must fetch fills for the income' in red.stdout, red.stdout
        assert 'could not compile' not in red.stdout and 'error[E' not in red.stdout, red.stdout
    finally:
        source.write_bytes(original)
    restored = run('exact-restoration')
    assert source.read_bytes() == original
    assert restored.returncode == 0 and f'test {name} ... ok' in restored.stdout, restored.stdout
finally:
    source.write_bytes(original)
print(json.dumps({'test': name, 'logs': str(logs),
                  'sourceSha256': hashlib.sha256(original).hexdigest(),
                  'namedCompiledFailure': True, 'exactRestorationPass': True}, indent=2))
