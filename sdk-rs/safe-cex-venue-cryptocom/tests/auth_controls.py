#!/usr/bin/env python3
"""Sequential compiling mutations; one Node corpus per test process, exact restoration."""
import os
from pathlib import Path
import subprocess
import sys
import json
ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom'
SOURCE = CRATE / 'src/signing.rs'
ORACLE = CRATE / 'tests/auth_oracle.cjs'
TEST = CRATE / 'tests/auth.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
COMMAND = ['cargo', 'test', '--manifest-path', str(ROOT/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-cryptocom', '--test', 'auth', '--jobs', '1', '--locked']
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
def swap_signature_numbers(text):
    left='safe_cex_num::js_number_string(id)';right='safe_cex_num::js_number_string(nonce)'
    assert text.count(left)==text.count(right)==1
    return text.replace(left,'__NUMBER_SWAP__').replace(right,left).replace('__NUMBER_SWAP__',right)
def bypass_clock_getter(text):
    assert text.count('clock.get_current_time_ms()') == 4
    return text.replace('clock.get_current_time_ms()', '(clock.now)()')
controls = [
 ('clock_getter_bypass', SOURCE, bypass_clock_getter, 'auth_native_id'),
 ('signature_without_admission', SOURCE, swap_signature_numbers, 'auth_native_signature:0'),
 ('request_params_projection', SOURCE, replace('        params,\n        sig,', '        params: params.property("a").unwrap_or_else(|| params.clone()),\n        sig,'), 'auth_native_params_identity'),
 ('signature_nonce_swap', SOURCE, swap_signature_numbers, 'auth_native_signature:0'),
 ('signature_extra', SOURCE, replace('    hmac_sha256(api_secret.as_bytes(), payload.as_bytes(), Encoding::Hex)', '    hmac_sha256(api_secret.as_bytes(), payload.as_bytes(), Encoding::Hex) + "extra"'), 'auth_native_signature:0'),
 ('request_nonce_eager', SOURCE, replace('nonce.unwrap_or_else(|| clock.get_current_time_ms())', 'nonce.unwrap_or(clock.get_current_time_ms())'), 'AUTH-EAGER-CLOCK-COUNT'),
 ('request_id_swap', SOURCE, replace('let request_nonce = nonce.unwrap_or_else(|| clock.get_current_time_ms());', 'let request_nonce = nonce.unwrap_or_else(|| clock.get_current_time_ms());\n    let (id, request_nonce) = (request_nonce, id);'), 'auth_native_id'),
 ('ws_nonce_reuse', SOURCE, replace('let nonce = clock.get_current_time_ms();', 'let nonce = id;'), 'AUTH-EAGER-CLOCK-COUNT'),
 ('error_name_flatten', SOURCE, replace('name: error.name.into()', 'name: "Error".into()'), 'auth_native_error_name'),
 ('source_same_length', ORACLE, replace("method:'private/create-order'", "method:'private/WRONG'"), 'auth_input_content'),
 ('source_reversal', ORACLE, replace('for(const input of inputs){', 'for(const input of [...inputs].reverse()){'), 'auth_input_content'),
 ('source_empty', ORACLE, replace('architecture:process.arch},cases}', 'architecture:process.arch},cases:[]}'), 'auth_source_count'),
 ('source_subset', ORACLE, replace('architecture:process.arch},cases}', 'architecture:process.arch},cases:cases.slice(1)}'), 'auth_source_count'),
 ('source_extra', ORACLE, replace('architecture:process.arch},cases}', 'architecture:process.arch},cases:[...cases,cases[0]]}'), 'auth_source_count'),
 ('comparison_subset', TEST, replace('for (index, case) in cases.iter().enumerate() {\n        for field', 'for (index, case) in cases.iter().take(41).enumerate() {\n        for field'), 'auth_executed_branches'),
]
def source_return(before,after):
    return replace("const source = fs.readFileSync(file, 'utf8');", "const source = fs.readFileSync(file, 'utf8').replace("+json.dumps(before)+", "+json.dumps(after)+");")
for name,before,after,assertion in [
 ('request_return_method', 'return {\n    id,\n    method,', 'return {\n    id,\n    method: "wrong",', 'auth_source_native_method'),
 ('request_return_key', 'api_key: apiKey,\n    params,', 'api_key: "wrong",\n    params,', 'auth_source_native_key'),
 ('ws_return_method', "method: 'public/auth',", "method: 'private/WRONG',", 'ws_source_native_method'),
 ('ws_return_key', 'api_key: apiKey,\n    sig:', 'api_key: "wrong",\n    sig:', 'ws_source_native_key'),
]: controls.append((name,ORACLE,source_return(before,after),assertion))
def remove_admission(text):
    start=text.index('        for field in ["input", "kind", "explicit", "clockValues"] {')
    end=text.index('        let input =',start)
    text=text[:start]+text[end:]
    marker='"auth_source_count"'
    end=text.index(');',text.index(marker))+2
    start=text.rfind('assert_eq!(',0,text.index(marker))
    return text[:start]+text[end:]
selected = set(sys.argv[1:])
assert not selected - {c[0] for c in controls}, 'unknown_auth_controls'
passed(run())
executed = []
for name, file, mutation, assertion in controls:
    if selected and name not in selected:
        continue
    original = file.read_bytes()
    test_original = TEST.read_bytes()
    try:
        if name == "signature_without_admission": TEST.write_text(remove_admission(test_original.decode()))
        file.write_text(mutation(original.decode()))
        result = run()
        print(result.stdout, flush=True)
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        file.write_bytes(original)
        if name == "signature_without_admission": TEST.write_bytes(test_original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
    executed.append(name)
assert set(executed) == (selected or {c[0] for c in controls}), 'auth_control_execution_count'
print(f'controls: {len(executed)} failures and restored passes', flush=True)
