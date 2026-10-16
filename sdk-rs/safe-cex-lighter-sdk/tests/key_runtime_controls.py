#!/usr/bin/env python3
"""Compiled SDK production mutations, each caught at a named oracle assertion."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-lighter-sdk', '--test', 'key_runtime', '--jobs', '1', '--locked']
runtime = 'key_runtime_matches_maintained_typescript'
records = 'signer_records_have_direct_native_fields'
controls = [
    ('readiness', 'key_runtime.rs', 'if !host.is_ready()? {', 'if host.is_ready()? {', runtime, 'SDK-KEY native output ensure 1'),
    ('key-predicate', 'key_runtime.rs', 'pub fn key_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {\n    if !host.is_object(value) || host.is_null(value) {\n        return Ok(false);\n    }\n    host.has_error(value)\n}',
     'pub fn key_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {\n    if !host.is_object(value) || host.is_null(value) {\n        return Ok(false);\n    }\n    host.has_error(value).map(|_| false)\n}', runtime, 'SDK-KEY native output generate 1'),
    ('signer-predicate', 'key_runtime.rs', 'pub fn signer_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {\n    if !host.is_object(value) || host.is_null(value) {\n        return Ok(false);\n    }\n    host.has_error(value)\n}',
     'pub fn signer_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {\n    if !host.is_object(value) || host.is_null(value) {\n        return Ok(false);\n    }\n    host.has_error(value).map(|_| false)\n}', runtime, 'SDK-KEY native output predicate-signer 0'),
    ('generation', 'key_runtime.rs', 'let private_key = host.property(&result, "privateKey")?;',
     'let private_key = host.property(&result, "publicKey")?;', runtime, 'SDK-KEY native output generate 0'),
    ('derivation', 'key_runtime.rs', 'if !valid_key(&private_key) {', 'if valid_key(&private_key) {', runtime, 'SDK-KEY native output derive 0'),
    ('signature', 'signing_types.rs', 'Self { tx_hash, signature }', 'Self { tx_hash: signature, signature: tx_hash }', records, 'SDK-KEY native signature hash'),
    ('auth-token', 'signing_types.rs', '            account_index,\n            api_key_index,',
     '            account_index: api_key_index,\n            api_key_index: account_index,', records, 'SDK-KEY native auth accountIndex'),
    ('error-class-constructor', 'signing_types.rs', 'name: "SignerError",', 'name: "WrongSignerError",',
     'signer_error_preserves_fields_and_cause_with_explicit_class_boundary', 'SDK-KEY native signer error name'),
]
selected = set(sys.argv[1:])
assert selected <= {c[0] for c in controls}, 'unknown family'
for name, file, before, after, test, assertion in controls:
    if selected and name not in selected:
        continue
    source = crate / 'src' / file
    original = source.read_text()
    assert original.count(before) == 1, f'{name}: unique production operation'
    try:
        source.write_text(original.replace(before, after))
        broken = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'lighter-sdk-key-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'{name}: {broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'lighter-sdk-key-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration {restored.stdout}'
    assert source.read_text() == original, 'production bytes restored'
    print(f'{name}: restored 0', flush=True)
