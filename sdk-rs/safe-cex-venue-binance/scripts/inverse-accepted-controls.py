#!/usr/bin/env python3
"""Compile isolated production-body faults, restore bytes, require native results."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
logs = workspace / 'target/inverse-accepted-controls'
logs.mkdir(parents=True, exist_ok=True)
runner = ['node', str(crate / 'scripts/inverse-accepted-contracts.mjs')]
results = workspace / 'target/inverse-accepted-results.json'
env = {**os.environ, 'CARGO_BUILD_JOBS': '1'}

def execute(label, names=None):
    args = runner + ([f'--only={",".join(names)}'] if names else [])
    with (logs / (label + '.log')).open('w') as output:
        run = subprocess.run(args, cwd=workspace, env=env, stdout=output, stderr=subprocess.STDOUT)
    return run.returncode

assert execute('baseline-green') == 0, 'control baseline must execute all 17 native owners'
inventory = json.loads((workspace / 'target/inverse-accepted-inventory.json').read_text())
files = {workspace / owner['span']['filename'] for owner in inventory['types']}
files.add(crate / 'src/inverse/exchange.rs')
originals = {p: p.read_bytes() for p in files}
receipt = {'controls': [], 'originalSha256': {str(p.relative_to(workspace)): hashlib.sha256(b).hexdigest() for p, b in originals.items()}}

def run_fault(label, file, changed, names):
    original = originals[file]
    assert file.read_bytes() == original
    try:
        file.write_text(changed)
        results.unlink(missing_ok=True)
        status = execute(label, names)
        assert status != 0, f'{label}: fault passed'
        assert results.exists(), f'{label}: no runtime receipts; compiler failures do not count'
        rows = json.loads(results.read_text())
        assert sorted(r['name'] for r in rows) == sorted(names), label
        assert all(r['failures'] and not r.get('classificationOnly') for r in rows), (label, rows)
        receipt['controls'].append({'control': label, 'failures': {r['name']: r['failures'] for r in rows}})
        print(label + ': compiled named native RED', flush=True)
    finally:
        file.write_bytes(original)
        assert file.read_bytes() == original
        (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')

for owner in inventory['types']:
    if owner['kind'] != 'function':
        continue
    file = workspace / owner['span']['filename']
    source = originals[file].decode()
    line, column = owner['span']['begin']
    start = sum(len(l) for l in source.splitlines(keepends=True)[:line - 1]) + column - 1
    opening = source.index('{', start)
    fault = '\nif std::hint::black_box(true) { return Err(safe_cex_exchange::js_runtime::value::Thrown::error("AcceptedFault", "isolated native owner control")); }\n'
    run_fault('function-' + owner['name'].split('::inverse::')[1].replace('::', '-'), file, source[:opening + 1] + fault + source[opening + 1:], [owner['name']])

file = crate / 'src/generated/binanceinverse.rs'
source = originals[file].decode()
for ident in ['BinanceIncomeRecord', 'BinanceInversePositionRisk']:
    pattern = ident + r'Wire::deserialize\(\s*safe_cex_types::strict_decode::object\(\s*decoder,?\s*\)\s*\)'
    changed, count = re.subn(pattern, ident + 'Wire::deserialize(decoder)', source)
    assert count == 1, (ident, count)
    names = ['safe_cex_venue_binance::binanceinverse_types::' + ident]
    if ident == 'BinanceInversePositionRisk':
        names.append('safe_cex_venue_binance::binanceinverse_types::GetPositionRiskResponse')
    run_fault('map-frontdoor-' + ident, file, changed, names)

file = crate / 'src/inverse/exchange.rs'
source = originals[file].decode()
for label, before, after in [
    ('generic-invoke', 'to_typed(&value).map_err(|e| e.to_exchange_error())', 'to_typed(&{ let _ = value; Js::Null }).map_err(|e| e.to_exchange_error())'),
    ('generic-listen', 'Ok(value) => callback(value),', 'Ok(value) => { let _ = (&callback, value); },'),
]:
    assert source.count(before) == 1, label
    run_fault(label, file, source.replace(before, after), ['safe_cex_venue_binance::inverse::BinanceInverseExchange'])

assert all(p.read_bytes() == b for p, b in originals.items())
assert execute('exact-restoration-green') == 0, 'exact restored production must pass all 17 owners'
receipt['restoredAll17Pass'] = True
(logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('All isolated compiled controls named RED; exact restoration passes all 17 native owners.', flush=True)
