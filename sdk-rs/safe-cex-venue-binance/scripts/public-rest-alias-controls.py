#!/usr/bin/env python3
"""Require compiled native public REST continuation failures and exact source restoration."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / 'src/exchange_classes.rs'
original = source.read_bytes()
logs = workspace / 'target/public-rest-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'production_native_public_rest_future_waits_and_preserves_candles_and_rejection'
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes', name, '--', '--exact']
receipt = {'sourceSha256': hashlib.sha256(original).hexdigest(), 'controls': []}


def execute(label):
    path = logs / (label + '.log')
    with path.open('w') as output:
        result = subprocess.run(command, cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


status, log = execute('baseline-green')
assert status == 0 and '1 passed; 0 failed' in log
for alias, before, after in [
    ('fulfilled', '            let v = request.await?;\n            if matches!(v, EventValue::Undefined) {',
     '            let _fulfilled = request.await?;\n            let v = EventValue::Undefined;\n            if matches!(v, EventValue::Undefined) {'),
    ('rejected', '            let v = request.await?;\n            if matches!(v, EventValue::Undefined) {',
     '            let v = request.await.unwrap_or_else(|_| EventValue::from_json(serde_json::json!([])));\n            if matches!(v, EventValue::Undefined) {'),
]:
    assert source.read_bytes() == original
    text = original.decode()
    assert text.count(before) == 1, alias
    try:
        source.write_text(text.replace(before, after))
        status, log = execute(alias + '-red')
        assert status != 0, alias + ': corrupted public REST continuation passed'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), alias + ': compiler errors do not count'
        assert f'test {name} ... FAILED' in log, alias + ': named runtime assertion must fail'
        assert '0 passed; 1 failed' in log
        receipt['controls'].append({'owner': 'safe_cex_venue_binance::public_rest::RestFuture', 'path': alias,
                                    'test': name, 'namedRuntimeFailure': True})
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(alias + '-exact-restoration-green')
    assert status == 0 and '1 passed; 0 failed' in log, alias
    print(alias + ': compiled named native RED; exact restoration GREEN', flush=True)
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
assert source.read_bytes() == original
print('Both public REST settlement controls fail by name and restore exactly.', flush=True)
