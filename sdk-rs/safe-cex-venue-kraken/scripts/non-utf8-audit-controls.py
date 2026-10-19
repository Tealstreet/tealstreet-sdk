#!/usr/bin/env python3
"""Fault the actual account-log binding; compiled assertions, then exact restores."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-kraken/src/binding/members.rs'
original = source.read_bytes()
env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(sdk.parent / 'node_modules/typescript'))
receipts = Path(tempfile.mkdtemp(prefix='kraken-non-utf8-controls-'))
name = 'binding::exchange::non_utf8_fix_audit::income_scalar_rows_survive_native_missing_and_nonfinite_cells'
date_name = 'binding::exchange::non_utf8_fix_audit::income_date_rows_skip_nonfinite_dates_and_keep_neighbors'
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken', '--lib', 'non_utf8_fix_audit::income_', '--', '--nocapture']
cases = [
    ('json-first', 'let logs = if data.is_nullish() {',
     'let _json = crate::wire::decode(number_input(&data)?.expect("native response"))\n                .map_err(|cause| Thrown::type_error(&cause.to_string()))?;\n            let logs = if data.is_nullish() {'),
    ('finite-guard', 'Ok(value.is_finite().then_some(value))', 'Ok(Some(value))'),
    ('date-unit', 'let timestamp = if timestamp.is_finite() {\n                        timestamp',
     'let timestamp = if timestamp.is_finite() {\n                        timestamp / 1000.0'),
]
results = []
for label, needle, fault in cases:
    text = original.decode()
    assert text.count(needle) == 1, label
    try:
        source.write_text(text.replace(needle, fault))
        red = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{label}-red.log').write_text(red.stdout)
        assert red.returncode and 'could not compile' not in red.stdout, red.stdout
        expected = [name, date_name] if label == 'json-first' else ([date_name] if label == 'date-unit' else [name])
        assert all(f'test {test} ... FAILED' in red.stdout for test in expected) and 'panicked at' in red.stdout, red.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    green = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (receipts / f'{label}-restored.log').write_text(green.stdout)
    assert green.returncode == 0 and all(f'test {test} ... ok' in green.stdout for test in [name, date_name]), green.stdout
    results.append({'control': label, 'red': red.returncode, 'restored': green.returncode,
                    'sourceSha256': hashlib.sha256(original).hexdigest(), 'tests': expected})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'{label}: named compiled RED, exact-source restoration GREEN', flush=True)
print(receipts, flush=True)
