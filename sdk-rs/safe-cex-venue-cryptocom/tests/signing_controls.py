#!/usr/bin/env python3
"""Sequential compiling mutations; one Node corpus per test process, exact restoration."""
import os
from pathlib import Path
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom'
SOURCE = CRATE / 'src/signing.rs'
ORACLE = CRATE / 'tests/signing_oracle.cjs'
TEST = CRATE / 'tests/signing.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-cryptocom', '--test', 'signing', '--jobs', '1', '--locked']
def run():
    return subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
def passed(result):
    assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, result.stdout
    print(result.stdout, flush=True)
def replace(before, after):
    def mutate(text):
        assert text.count(before) == 1, (before, text.count(before))
        return text.replace(before, after)
    return mutate
def increment(text):
    assert text.count('level + 1.0') == 2
    return text.replace('level + 1.0', 'level + 2.0')
controls = [
    ('source_missing_raw', ORACLE, replace('console.log(JSON.stringify', 'delete rows[0].raw; console.log(JSON.stringify'), 'params_raw_presence'),
    ('source_missing_level', ORACLE, replace('console.log(JSON.stringify', 'delete rows[0].level; console.log(JSON.stringify'), 'params_level_presence'),
    ('source_content_replaced', ORACLE, replace('["first",null,2,"last"]', '["changed",null,2,"last"]'), 'params_input_content'),
    ('source_content_reversal', ORACLE, replace('for (const [name, raw] of inputs)', 'for (const [name, raw] of [...inputs].reverse())'), 'params_input_content'),
    ('child_level', SOURCE, increment, 'params_native_value:nested_array'),
    ('depth_cutoff', SOURCE, replace('level >= 3.0', 'level > 3.0'), 'params_native_value:array'),
    ('utf8_sort', SOURCE, replace('a.encode_utf16().cmp(b.encode_utf16())', 'a.cmp(b)'), 'params_native_value:utf16_keys'),
    ('array_reverse', SOURCE, replace('for item in values {', 'for item in values.iter().rev() {'), 'params_native_value:array'),
    ('array_empty', SOURCE, replace('for item in values {', 'for item in values.iter().take(0) {'), 'params_native_value:array'),
    ('array_subset', SOURCE, replace('for item in values {', 'for item in values.iter().skip(1) {'), 'params_native_value:array'),
    ('object_empty', SOURCE, replace('for key in keys {', 'for key in keys.into_iter().take(0) {'), 'params_native_value:sorted_keys'),
    ('object_subset', SOURCE, replace('for key in keys {', 'for key in keys.into_iter().skip(1) {'), 'params_native_value:sorted_keys'),
    ('null_empty', SOURCE, replace('Some(Value::Null) => Ok("null".into())', 'Some(Value::Null) => Ok(String::new())'), 'params_native_value:null'),
    ('source_empty', ORACLE, replace('rows, excluded}', 'rows: [], excluded}'), 'params_source_count'),
    ('source_subset', ORACLE, replace('rows, excluded}', 'rows: rows.slice(1), excluded}'), 'params_source_count'),
    ('walk_subset', TEST, replace('rows.iter().enumerate()', 'rows.iter().take(279).enumerate()'), 'params_comparisons_executed'),
]
selected = set(sys.argv[1:])
assert not selected - {c[0] for c in controls}, 'unknown_signing_controls'
passed(run())
executed = []
for name, file, mutation, assertion in controls:
    if selected and name not in selected:
        continue
    original = file.read_bytes()
    try:
        file.write_text(mutation(original.decode()))
        result = run()
        print(result.stdout, flush=True)
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        file.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
    executed.append(name)
assert set(executed) == (selected or {c[0] for c in controls}), 'signing_control_execution_count'
print(f'controls: {len(executed)} failures and restored passes', flush=True)
