"""Serial empty/subset controls, with source-count bypass independence proofs."""
from pathlib import Path
import json
import os
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
rust = crate / 'src/types.rs'
constants = crate / 'src/constants.rs'
oracle = crate / 'tests/auth_types_oracle.cjs'
harness = crate / 'tests/auth_types.rs'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
controls = []

def replace(old, new):
    def change(text):
        assert text.count(old) == 1, old
        return text.replace(old, new)
    return change

def add(name, target, assertion, *changes):
    controls.append((name, target, assertion, changes))

book = 'order_book_record_actual_typescript'
response = 'order_response_record_actual_typescript'
numeric = 'numeric_auth_fields_before_serialization'
for side in ['bids', 'asks']:
    for count in [0, 1]:
        add(f'book-{side}-take-{count}', book, f'native_book.{side}.length',
            (rust, replace(f'{side}: fields.{side},', f'{side}: fields.{side}.into_iter().take({count}).collect(),')))
for count in [0, 1]:
    add(f'response-hashes-take-{count}', response, 'native_order_response.transactions_hashes',
        (rust, replace('transactions_hashes: fields.transactions_hashes,', f'transactions_hashes: fields.transactions_hashes.map(|v| v.into_iter().take({count}).collect()),')))
add('constant-array-empty', 'constants', 'constant:EIP712_ORDER_TYPES:array_length',
    (constants, lambda s: re.sub(r'(pub const EIP712_ORDER_TYPES:[\s\S]*?= )&\[\([\s\S]*?\)\];', r'\1&[("Order", &[])];', s, count=1)))
add('constant-array-subset', 'constants', 'constant:EIP712_ORDER_TYPES:array_length',
    (constants, replace('        TypedDataField {\n            name: "salt",\n            field_type: "uint256",\n        },\n', '')))
add('constant-map-empty', 'constants', 'constant:ENDPOINTS:own_keys',
    (constants, lambda s: re.sub(r'(pub const ENDPOINTS:[\s\S]*?= )&\[[\s\S]*?\];', r'\1&[];', s, count=1)))
add('constant-map-subset', 'constants', 'constant:ENDPOINTS:own_keys',
    (constants, replace('    ("MARKETS", "/markets"),\n', '')))
for name, statement in [('empty', 'numeric.length = 0;'), ('subset', 'numeric.pop();')]:
    add('numeric-source-'+name, numeric, 'auth_numeric_source_count',
        (oracle, replace('  process.stdout.write(', f'  {statement}\n  process.stdout.write(')))
for side in ['bids', 'asks']:
    add(f'book-source-{side}-subset', book, f'book_source.{side}.length',
        (oracle, replace('  await actualBook(bookSample);', f'  bookSample.{side}.pop();\n  await actualBook(bookSample);')))

bypass_book = lambda s: re.sub(r'fn assert_book_source_count\(source: &Value, expected: usize\) \{[\s\S]*?\n\}', 'fn assert_book_source_count(_source: &Value, _expected: usize) {}', s, count=1)
add('independent-native-bids-empty', book, 'native_book.bids.length',
    (harness, bypass_book), (rust, replace('bids: fields.bids,', 'bids: fields.bids.into_iter().take(0).collect(),')))
add('independent-numeric-empty', numeric, 'auth_numeric_native_comparisons',
    (harness, replace('    assert_eq!(cases.len(), 9, "auth_numeric_source_count");\n', '')),
    (oracle, replace('  process.stdout.write(', '  numeric.length = 0;\n  process.stdout.write(')))
add('independent-book-source-subset', book, 'book_total_native_level_comparisons',
    (harness, bypass_book),
    (oracle, replace('  await actualBook(bookSample);', '  bookSample.bids.pop();\n  await actualBook(bookSample);')))

def run(name, target):
    command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'constants' if target == 'constants' else 'auth_types', '--locked', '--jobs', '1', '--']
    if target != 'constants': command.append(target)
    command.append('--nocapture')
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
    if target != 'constants':
        assert result.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, result.stdout
    return result

summary = {'sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=crate, text=True).strip(), 'controls': []}
for name, target, assertion, changes in controls:
    if os.environ.get("POLYMARKET_CONTROL_FILTER", "") not in name: continue
    originals = {file: file.read_text() for file, _ in changes}
    try:
        for file, change in changes:
            original = file.read_text(); changed = change(original)
            assert changed != original, name
            file.write_text(changed)
        result = run(name+'-mutation', target)
        assert result.returncode == 101 and assertion in result.stdout, result.stdout
    finally:
        for file, original in originals.items(): file.write_text(original)
    restored = run(name+'-restored', target)
    assert restored.returncode == 0, restored.stdout
    summary['controls'].append({'name': name, 'mutation_exit': result.returncode, 'assertion': assertion, 'restored_exit': restored.returncode})
    (logs/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(f'{name}: 101 {assertion}; restored 0', flush=True)
