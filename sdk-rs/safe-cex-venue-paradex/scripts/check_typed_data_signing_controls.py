"""Compiled production controls: require the named native assertion, then a restore pass."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-paradex/src/typed_data_signing.rs'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'typed_data_signing', '--jobs', '1']
ENV = dict(os.environ)
LOGS = ROOT / '.aimux/typed-data-signing-controls'
LOGS.mkdir(parents=True, exist_ok=True)
original = SOURCE.read_text()
controls = [
    ('remote-key', '("key", key)', '("key", {drop(key);h.literal("wrong-key")})', 'remote_native_request'),
    ('remote-receiver', 'h.call(signer, options, vec![request])', 'h.call(signer, sdk_account, vec![request])', 'remote_native_receiver'),
    ('account-address', '"address")?', '"signMessage")?', 'hash_native_address'),
    ('local-input', 'h.call(sign_message, &account, vec![typed_data.clone()])', 'h.call(sign_message, &account, vec![options.clone()])', 'local_native_input'),
    ('bigint-receiver', 'h.call(method, &value, vec![])', 'h.call(method, sdk_account, vec![])', 'bigint_native_receiver'),
    ('felt-pair-order', 'h.array(vec![r, s])', 'h.array(vec![s, r])', 'signature_native_value'),
    ('bigint-conversion', 'if h.is_bigint(&value)', 'if false && h.is_bigint(&value)', 'signing_native_order'),
    ('native-address', 'parse_felt(account_address)?', 'parse_felt({let _retained=account_address; "0x9999"})?', 'crypto_adapter_native_request'),
    ('native-key', 'parse_felt(private_key)?', 'parse_felt({let _retained=private_key; account_address})?', 'crypto_adapter_native_signature'),
]

def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, text=True, capture_output=True)
    output = result.stdout + result.stderr
    (LOGS / (label + '.log')).write_text(output)
    return result.returncode, output

head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
code, output = run('baseline')
assert code == 0, output
report = {'head': head, 'baseline_exit': code, 'controls': []}
for name, needle, replacement, assertion in controls:
    expected_count = 2 if name == 'native-address' else 1
    assert original.count(needle) == expected_count, (name, original.count(needle))
    changed = original.replace(needle, replacement)
    try:
        SOURCE.write_text(changed)
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
assert len(report['controls']) == 9
assert SOURCE.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
