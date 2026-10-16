"""Compiled enum controls require the declared native assertion and a passing restore."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-crypto/src/starknet.rs'
LOGS = ROOT / '.aimux/stark-empty-controls'
LOGS.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ)
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked',
           '-p', 'safe-cex-crypto', '--test', 'stark_empty_tuples',
           '--test', 'stark_empty_slots', '--test', 'stark_unused_enum', '--jobs', '1']
controls = [
    ('revision-specific-quoting', 'let active = revision == Revision::V1;',
     'let active = revision == Revision::V1 || revision == Revision::V0;', 'unused_enum_native_hash'),
    ('empty-zero-word', 'encoded.push(word(Felt::ZERO));',
     'encoded.push(word(Felt::ONE));', 'empty_tuple_native_hash scalar/Zero'),
    ('variant-index', 'vec![InlineTypeReference::Felt; encoded.len()];',
     'vec![InlineTypeReference::Felt; encoded.len()];\n                    synthetic.variants.swap(0, 2);',
     'empty_tuple_native_hash scalar/Zero'),
    ('original-raw-type-hash', 'let original_hash = sdk_type_hash(raw_types, name, encoder.revision())?;',
     'let _retained_raw_hash = sdk_type_hash(raw_types, name, encoder.revision())?;\n'
     '                    let original_hash = encoder.types().get_type_hash(name).map_err(|error| error.to_string())?;',
     'empty_slot_native_hash'),
    ('ignored-empty-slot', 'let mut element = if kind.is_empty() {\n                serde_json::json!("0")',
     'let mut element = if kind.is_empty() {\n                serde_json::json!("1")', 'empty_slot_native_hash'),
    ('array-order', '            encoder\n                .encode_value(\n                    &FullTypeReference::Array(ElementTypeReference::Felt),',
     '            encoded.reverse();\n            encoder\n                .encode_value(\n                    &FullTypeReference::Array(ElementTypeReference::Felt),',
     'empty_tuple_native_hash array/Zero'),
    ('single-merkle-leaf', 'if values.elements.len() == 1 {',
     'if values.elements.len() == usize::MAX {', 'empty_tuple_native_hash merkle1/Zero'),
    ('merkle-leaf-composition', 'CommonTypeReference::MerkleTree(element) => {',
     'CommonTypeReference::MerkleTree(element) => {\n'
     '            if depth != usize::MAX { return encoder.encode_value(reference, value).map_err(|error| error.to_string()); }', 'empty_tuple_native_hash merkle1/Zero'),
]

def run(label, target=None):
    command = COMMAND if target is None else COMMAND[:7] + ['--test', target, '--jobs', '1']
    result = subprocess.run(command, cwd=ROOT, env=ENV, capture_output=True, text=True)
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
        target = 'stark_unused_enum' if name == 'revision-specific-quoting' else ('stark_empty_slots' if name in ('original-raw-type-hash', 'ignored-empty-slot') else 'stark_empty_tuples')
        code, output = run(name, target)
    finally:
        SOURCE.write_text(original)
    assert code == 101 and 'test result: FAILED' in output, (name, code, output)
    assert assertion in output and 'could not compile' not in output, (name, assertion, output)
    restored, restore_output = run(name + '-restore', target)
    assert restored == 0, (name, restored, restore_output)
    row = {'name': name, 'assertion': assertion, 'mutation_exit': code, 'restore_exit': restored}
    report['controls'].append(row)
    (LOGS / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 8
assert SOURCE.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
