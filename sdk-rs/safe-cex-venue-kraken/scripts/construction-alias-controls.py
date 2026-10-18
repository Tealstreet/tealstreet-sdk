#!/usr/bin/env python3
"""Compiled faults in the live constructor's Factory and RateLimit consumers."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-construction-alias-controls-'))
path = sdk / 'safe-cex-venue-kraken/src/construction.rs'
cases = [
    ('Factory', 'factory_alias_selects_source_bases_and_real_signed_client_pipeline',
     'h.create_api(venue, options, receiver, base)',
     'h.create_api(venue, options, receiver, { let _ = base; BaseUrl::History })'),
    ('RateLimit', 'rate_limit_alias_and_constructor_enforce_source_five_request_window',
     'h.rate_limit(client, 5.0)', 'h.rate_limit(client, 4.0)'),
]
results = []
for alias, test, needle, replacement in cases:
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_construction_aliases::{test}'
    def run(label, negative):
        result = subprocess.run([
            'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken',
            '--lib', name, '--', '--exact', '--nocapture',
        ], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{alias}-{label}.log').write_text(result.stdout)
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
    results.append({'alias': alias, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'{alias}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
