#!/usr/bin/env python3
"""Two actual production-body faults, named native failures, exact restoration."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-binance/src/public_subscriptions.rs'
original = source.read_bytes()
test = 'public_subscription_receipts::production_native_subscription_frames_order_counts_reentry_and_send_failure'
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--test', 'private_classes', test, '--', '--exact']
directory = Path(tempfile.mkdtemp(prefix='binance-subscription-controls-'))
receipts = []


def run(label):
    result = subprocess.run(command, cwd=sdk, env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (directory / (label + '.log')).write_text(result.stdout)
    return result


faults = [
    ('subscribe_binance_public', b'    subscribe(keys, count, send)',
     b'    subscribe(keys.into_iter().take(0), count, send)'),
    ('subscribe', b'let mut id = 10.0;', b'let mut id = 11.0;'),
]
try:
    baseline = run('baseline')
    assert baseline.returncode == 0 and f'test {test} ... ok' in baseline.stdout, baseline.stdout
    for name, old, new in faults:
        assert original.count(old) == 1, (name, 'mutation must name exactly one body')
        source.write_bytes(original.replace(old, new))
        try:
            red = run(name + '-red')
            assert red.returncode != 0 and f'test {test} ... FAILED' in red.stdout, red.stdout
            assert 'assertion `left == right` failed: snapshot keys, fresh finite counts and exact outgoing bytes' in red.stdout, red.stdout
            assert 'error[E' not in red.stdout and 'could not compile' not in red.stdout, red.stdout
        finally:
            source.write_bytes(original)
        green = run(name + '-restored')
        assert source.read_bytes() == original
        assert green.returncode == 0 and f'test {test} ... ok' in green.stdout, green.stdout
        receipts.append({'owner': name, 'compiledNamedFailure': True, 'exactRestoredPass': True})
finally:
    source.write_bytes(original)
print(json.dumps({'logs': str(directory), 'receipts': receipts}, indent=2))
