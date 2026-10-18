#!/usr/bin/env python3
"""Require the constructed symbol-cancel witness to detect a broken handle filter."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/cancel_symbol_orders.rs'
original = source.read_bytes()
before = b'                if order.symbol != symbol {\n'
after = b'                if order.symbol == symbol {\n'
name = ('private_order_aliases::'
        'native_cancel_symbol_handles_wait_for_both_signed_deletes_and_keep_other_symbols')
logs = crate.parent / 'target/cancel-order-handle-controls'
logs.mkdir(parents=True, exist_ok=True)


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


assert original.count(before) == 1
try:
    source.write_bytes(original.replace(before, after))
    status, log = execute('symbol-filter-red')
    assert status != 0 and f'test {name} ... FAILED' in log, log[-5000:]
    assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), log[-5000:]
    assert 'cancel-symbol handle status for' in log, log[-5000:]
finally:
    source.write_bytes(original)
    assert source.read_bytes() == original
status, log = execute('symbol-filter-restored')
assert status == 0 and '1 passed; 0 failed' in log, log[-5000:]
print('symbol-filter: compiled assertion RED; exact restoration GREEN.', flush=True)
