"""Compiled SDK encoding controls require their native assertion and a passing restore."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-crypto/src/starknet.rs'
LOGS = ROOT / '.aimux/stark-sdk-encoding-controls'
LOGS.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ)
PREFIX = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked',
          '-p', 'safe-cex-crypto']
TARGETS = ['stark_selectors', 'stark_text_encoding', 'stark_enum_entries', 'stark_domain_revision']
controls = [
    ('selector-default-name-hash',
     'Ok(starknet_core::utils::starknet_keccak(text.as_bytes()))',
     '{ let _retained_sdk_hash = starknet_core::utils::starknet_keccak(text.as_bytes()); '
     'starknet_core::utils::get_selector_from_name(text).map_err(|error| error.to_string()) }',
     'selector_native_hash 0/default', 'stark_selectors', 'actual_sdk_selector_names_and_hex'),
    ('selector-coercion', 'Kind::Selector => {',
     'Kind::Selector => {\n'
     '                if !value.is_string() { return Err("Selector control requires string".into()); }',
     'selector_coercion_native_accepted 0/0', 'stark_selectors', 'actual_sdk_selector_json_coercion'),
    ('selector-hex-reduction',
     '                let (_, bytes) = number.to_bytes_be();\n'
     '                Ok(Felt::from_bytes_be_slice(&bytes))',
     '                let (_, bytes) = number.to_bytes_be();\n'
     '                Ok(Felt::from_bytes_be_slice(&bytes) + Felt::ONE)',
     'selector_native_hash 0/hex', 'stark_selectors', 'actual_sdk_selector_names_and_hex'),
    ('shortword-nibbles', 'hex.push_str(&format!("{byte:x}"));',
     'hex.push_str(&format!("{byte:02x}"));',
     'text_native_hash 0/felt/nul', 'stark_text_encoding', 'actual_sdk_text_word_encoding'),
    ('bytearray-pending-length', 'words.push(Felt::from(remainder.len()));',
     'words.push(Felt::from(remainder.len() + 1));',
     'text_native_hash 1/string/empty', 'stark_text_encoding', 'actual_sdk_text_word_encoding'),
    ('enum-first-entry',
     'safe_cex_num::normalize_object_order(object);\n'
     '                object\n                    .iter()\n                    .next()',
     'safe_cex_num::normalize_object_order(object);\n'
     '                object\n                    .iter()\n                    .next_back()',
     'enum_entries_native_hash extra-known', 'stark_enum_entries', 'actual_sdk_enum_entry_selection_and_payloads'),
    ('original-domain-revision', 'if !(active_revision || legacy_revision) {',
     'if false && !(active_revision || legacy_revision) {',
     'domain_revision_native_refusal 0/string-one', 'stark_domain_revision', 'actual_sdk_original_domain_revision_recognition'),
]


def run(label, target=None, test=None):
    command = list(PREFIX)
    for name in TARGETS if target is None else [target]:
        command += ['--test', name]
    command += ['--jobs', '1']
    if test is not None:
        command += [test, '--', '--exact']
    result = subprocess.run(command, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / (label + '.log')).write_text(output)
    return result.returncode, output


original = SOURCE.read_bytes()
source_text = original.decode()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
code, output = run('baseline')
assert code == 0, output
report = {'head': head, 'baseline_exit': code, 'controls': []}
(LOGS / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
for name, needle, replacement, assertion, target, test in controls:
    assert source_text.count(needle) == 1, (name, source_text.count(needle))
    assert SOURCE.read_bytes() == original, name
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    try:
        SOURCE.write_bytes(source_text.replace(needle, replacement).encode())
        code, output = run(name, target, test)
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original, name
    assert code == 101 and 'test result: FAILED' in output, (name, code, output)
    assert assertion in output and 'could not compile' not in output, (name, assertion, output)
    restored, restore_output = run(name + '-restore', target, test)
    assert restored == 0, (name, restored, restore_output)
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    row = {'name': name, 'target': target, 'test': test, 'assertion': assertion,
           'mutation_exit': code, 'restore_exit': restored}
    report['controls'].append(row)
    (LOGS / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 7
assert SOURCE.read_bytes() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
