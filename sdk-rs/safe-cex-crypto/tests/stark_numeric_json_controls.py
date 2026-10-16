"""Compiled Stark controls require their named runtime assertion and a restore pass."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-crypto/src/starknet.rs'
LOGS = ROOT / '.aimux/stark-json-controls'
LOGS.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ)
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-crypto', '--test', 'stark_numeric_json', '--jobs', '1']
controls = [
    ('json-number-rounding', 'fn check_felt(value: &mut serde_json::Value) -> Result<(), String> {',
     'fn check_felt(value: &mut serde_json::Value) -> Result<(), String> {\n'
     '    if value.is_number() { *value = serde_json::Value::String(value.to_string()); }',
     'numeric_json_native_hash 0/scalar/9007199254740993', 'actual_sdk_raw_json_number_ingress'),
    ('json-i128-rounding', 'Kind::I128 if revision_one => {',
     'Kind::I128 if revision_one => {\n'
     '                if value.is_number() { *value = serde_json::Value::String(value.to_string()); }',
     'numeric_json_native_hash 1/i128/9007199254740993', 'actual_sdk_raw_json_number_ingress'),
    ('enum-i128-normalization', 'if !kind.is_empty() {\n                            normalize_declared_message(types, kind, None, value)?;',
     'if kind.is_empty() {\n                            normalize_declared_message(types, kind, None, value)?;',
     'numeric_json_recursive_native_accepted i128/enum/number', 'actual_sdk_recursive_numeric_json_ingress'),
    ('declared-preset-projection', 'object.retain(|key, _| fields.iter().any(|field| field["name"] == key.as_str()));',
     'if depth == usize::MAX { object.retain(|key, _| fields.iter().any(|field| field["name"] == key.as_str())); }',
     'numeric_json_recursive_native_accepted u256/root-extra', 'actual_sdk_recursive_numeric_json_ingress'),
]

def run(label, test=None):
    command = COMMAND if test is None else COMMAND + [test]
    result = subprocess.run(command, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / (label + '.log')).write_text(output)
    return result.returncode, output

original = SOURCE.read_text()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
code, output = run('baseline')
assert code == 0, output
report = {'head': head, 'baseline_exit': code, 'controls': []}
for name, needle, replacement, assertion, test in controls:
    assert original.count(needle) == 1, (name, original.count(needle))
    try:
        SOURCE.write_text(original.replace(needle, replacement))
        code, output = run(name, test)
    finally:
        SOURCE.write_text(original)
    assert code == 101 and 'test result: FAILED' in output, (name, code, output)
    assert assertion in output and 'could not compile' not in output, (name, assertion, output)
    restored, restore_output = run(name + '-restore', test)
    assert restored == 0, (name, restored, restore_output)
    row = {'name': name, 'assertion': assertion, 'mutation_exit': code, 'restore_exit': restored}
    report['controls'].append(row)
    (LOGS / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 4
assert SOURCE.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
