#!/usr/bin/env python3
"""Compile independent query regressions and require their named assertions."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
query = root / 'sdk-rs/safe-cex-transport/src/query.rs'
signing = root / 'sdk-rs/safe-cex-transport/src/query_signing.rs'
cargo = os.environ.get('CARGO', 'cargo')
base = [cargo, 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '--locked', '--jobs', '1', '-p', 'safe-cex-transport', '--test', 'query_params']
controls = [
    ('zero-sign-erased', query, '(QueryValue::Number(left), QueryValue::Number(right)) => left.to_bits() == right.to_bits(),', '(QueryValue::Number(left), QueryValue::Number(right)) => (*left == 0.0 && *right == 0.0) || left.to_bits() == right.to_bits(),', 'retry_identity_retains_nan_and_distinguishes_zero_sign', 'QUERY retry signed zero identity'),
    ('finite-identity-erased', query, '(QueryValue::Number(left), QueryValue::Number(right)) => left.to_bits() == right.to_bits(),', '(QueryValue::Number(left), QueryValue::Number(right)) => (left.is_finite() && right.is_finite() && *left != 0.0 && *right != 0.0) || left.to_bits() == right.to_bits(),', 'retry_identity_retains_nan_and_distinguishes_zero_sign', 'QUERY retry finite identity'),
    ('numeric-json-projection', query, 'QueryValue::Number(value) => safe_cex_num::js_number_string(*value),', 'QueryValue::Number(_value) => "null".into(),', 'source_query_profiles_preserve_native_values_and_exact_wire', 'QUERY exact source rendering 0'),
    ('repeat-becomes-indexed', query, 'QueryArrayFormat::Repeat => key.into(),', 'QueryArrayFormat::Repeat => format!("{key}[{index}]"),', 'source_query_profiles_preserve_native_values_and_exact_wire', 'QUERY exact source rendering 3'),
]
for name, path, before, after, test, assertion in controls:
    original = path.read_bytes()
    text = original.decode()
    if text.count(before) != 1:
        raise SystemExit(f'{name}: mutation site drift')
    try:
        path.write_text(text.replace(before, after))
        result = subprocess.run(base + [test, '--', '--exact'], cwd=root, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != 101 or assertion not in output or 'error[E' in output:
            raise SystemExit(f'{name}: missing compiled named failure\n{output}')
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        path.write_bytes(original)
    restored = subprocess.run(base + [test, '--', '--exact'], cwd=root, capture_output=True, text=True)
    if restored.returncode:
        raise SystemExit(restored.stdout + restored.stderr)
    print(f'{name}: restored pass', flush=True)

original = signing.read_bytes()
text = original.decode()
start = text.index('pub struct SignedConfig')
end = text.index('pub struct HttpSigningResponse', start)
part = text[start:end]
before = 'skip_serializing_if = "query_wire_omitted",'
if part.count(before) != 1:
    raise SystemExit('response-undefined: mutation site drift')
try:
    signing.write_text(text[:start] + part.replace(before, '') + text[end:])
    result = subprocess.run(base + ['signing_wire_preserves_numeric_tokens_and_distinct_omissions', '--', '--exact'], cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    if result.returncode != 101 or 'QUERY signing wire omission identity' not in output or 'error[E' in output:
        raise SystemExit(f'response-undefined: missing compiled named failure\n{output}')
    print('response-undefined: compiled 101 at QUERY signing wire omission identity', flush=True)
finally:
    signing.write_bytes(original)
restored = subprocess.run(base, cwd=root, capture_output=True, text=True)
if restored.returncode:
    raise SystemExit(restored.stdout + restored.stderr)
print('all query controls restored; query_params passed', flush=True)
