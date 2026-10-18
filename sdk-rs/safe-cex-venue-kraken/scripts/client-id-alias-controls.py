#!/usr/bin/env python3
"""Named compiled faults in the production Spot client-ID consumer."""
from pathlib import Path
import json
import os
import subprocess
import tempfile

os.environ['CARGO_INCREMENTAL'] = '0'
sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-client-id-alias-controls-'))
path = sdk / 'safe-cex-venue-kraken/src/binding/host.rs'
cases = [
    ('output', 'spot_ids_share_real_entropy_pool_across_constructed_placement_clients',
     'Ok(h(Js::str(&id)))', 'Ok(h(Js::str(&{ let _ = id; "0".repeat(32) })))'),
    ('error', 'spot_ids_entropy_failure_reaches_native_host_and_prevents_order_http',
     '(self.shared.extra.spot_ids.borrow_mut())().map_err(Thrown::from)?',
     '(self.shared.extra.spot_ids.borrow_mut())().unwrap_or_else(|_| "0".repeat(32))'),
]
results = []
for label, test, needle, replacement in cases:
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_client_id_alias::{test}'
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
