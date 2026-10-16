"""Compiled Stark controls require their named runtime assertion and a restore pass."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-crypto/src/starknet.rs'
LOGS = ROOT / '.aimux/stark-string-controls'
LOGS.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ)
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-crypto', '--test', 'stark_numeric_strings', '--jobs', '1']
normalization = '''    if let Ok(number) = safe_cex_num::js_bigint(Some(value)) {
        *value = serde_json::Value::String(number.to_string());
    }
'''
controls = [
    ('numeric-normalization', normalization, '', 'numeric_string_native_hash'),
    ('original-domain-type-hash', 'format!("StarkNetDomain({fields})").as_bytes()',
     '{let _retained=&fields; b"StarkNetDomain(name:felt,version:felt,chainId:felt)"}',
     'domain_native_original_hash'),
    ('original-domain-field-order', 'serde_json::from_value(types["StarkNetDomain"].clone())',
     'serde_json::from_value(canonical.clone())', 'domain_native_original_hash'),
    ('original-account-prime', 'pub fn parse_felt(input: &str) -> Result<Felt, String> {',
     'pub fn parse_felt(input: &str) -> Result<Felt, String> {\n'
     '    if input == "0x800000000000011000000000000000000000000000000000000000000000001" { return Ok(Felt::ZERO); }',
     'account_native_refusal'),
]

def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / (label + '.log')).write_text(output)
    return result.returncode, output

original = SOURCE.read_text()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
code, output = run('baseline')
assert code == 0, output
report = {'head': head, 'baseline_exit': code, 'controls': []}
for name, needle, replacement, assertion in controls:
    assert original.count(needle) == 1, (name, original.count(needle))
    try:
        SOURCE.write_text(original.replace(needle, replacement))
        code, output = run(name)
    finally:
        SOURCE.write_text(original)
    assert code == 101 and 'test result: FAILED' in output, (name, code, output)
    assert assertion in output and 'could not compile' not in output, (name, assertion, output)
    restored, restore_output = run(name + '-restore')
    assert restored == 0, (name, restored, restore_output)
    row = {'name': name, 'assertion': assertion, 'mutation_exit': code, 'restore_exit': restored}
    report['controls'].append(row)
    (LOGS / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 4
assert SOURCE.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
