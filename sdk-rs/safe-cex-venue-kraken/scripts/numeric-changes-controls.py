#!/usr/bin/env python3
"""Named compiled faults in the production NumericChanges consumers."""
from pathlib import Path
import json
import os
import subprocess
import tempfile

os.environ['CARGO_INCREMENTAL'] = '0'
sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-numeric-changes-controls-'))
path = sdk / 'safe-cex-venue-kraken/src/stream_updates.rs'
cases = [
    ('futures-patch-key', 'numeric_changes_reaches_real_futures_socket_store_with_each_key_and_bit',
     '("bid", bid),', '("ask", bid),'),
    ('spot-patch-key', 'numeric_changes_reaches_real_spot_socket_store_with_each_key_and_bit',
     'changes.push(("volume", value));', 'changes.push(("quoteVolume", value));'),
]
results = []
for label, test, needle, replacement in cases:
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_numeric_changes::{test}'
    def run(stage, negative):
        result = subprocess.run([
            'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken',
            '--lib', name, '--', '--exact', '--nocapture',
        ], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{label}-{stage}.log').write_text(result.stdout)
        if negative:
            assert result.returncode != 0 and f'test {name} ... FAILED' in result.stdout and 'panicked at' in result.stdout and 'could not compile' not in result.stdout, result.stdout
        else:
            assert result.returncode == 0 and f'test {name} ... ok' in result.stdout, result.stdout
        return result.returncode
    try:
        path.write_text(source.replace(needle, replacement))
        red = run('red', True)
    finally:
        path.write_bytes(original)
    green = run('restored', False)
    results.append({'control': label, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'{label}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
