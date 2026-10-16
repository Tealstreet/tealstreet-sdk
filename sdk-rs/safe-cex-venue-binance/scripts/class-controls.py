#!/usr/bin/env python3
"""Run class-family mutations serially and restore every edited input in finally."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-binance'
CARGO = os.environ.get('CARGO', 'cargo')
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
OUT = CRATE / 'gate-evidence/class-owners'
OUT.mkdir(parents=True, exist_ok=True)


def run(test_file, test, env):
    args = [CARGO, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
            'safe-cex-venue-binance', '--test', test_file, '--jobs', '2',
            test, '--', '--exact', '--include-ignored', '--nocapture']
    result = subprocess.run(args, cwd=ROOT, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return result.returncode, result.stdout


controls = [
    ('rust-dispatch', 'src/public_dispatch.rs',
     'let json = crate::private_session::parse_data(string(data));',
     'let json = crate::private_session::parse_data(string("{}"));',
     'class_dispatch', 'dispatch_native_overflow_signed_zero_and_deep_frame',
     'native JSON overflow is Infinity'),
    ('rust-public-alias', 'src/ticker_capture.rs',
     'dictionary.insert(symbol.clone(), reference.clone());',
     'let _ = (&mut dictionary, symbol, reference);', 'public_classes', 'public_same_generation_symbol_alias_receives_second_tick',
     'native same-generation alias receives second price 12.3 rather than 4.2'),
    ('rust-private-open', 'src/private_classes.rs',
     '                    owner.on_open_sync()\n',
     '                    class_jobs::detach(owner.runtime.clone(), owner.on_open());\n                    Ok(())\n',
     'private_classes', 'private_registered_open_propagates_synchronous_original_error',
     'native synchronous open callback must propagate failure'),
    ('rust-exchange-client', 'src/private_session.rs',
     'b.api(v, Factory::FAPI, opts.clone(), owner.clone())?',
     'b.api(v, Factory::PAPI, opts.clone(), owner.clone())?',
     'private_classes', 'exchange_owners_bind_real_children_clients_and_native_state_before_source',
     'native exchange constructor client route binancepm.fapiXhr'),
    ('rust-detached-job', 'src/class_jobs.rs',
     'if state.observed || state.report_scheduled',
     'if !state.observed || state.report_scheduled',
     'class_jobs', 'retained_unpolled_rejection_reports_before_handle_release',
     'native retained unobserved rejection reports before release'),
]

for name, relative, old, new, target, test, assertion in controls:
    path = CRATE / relative
    original = path.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, (name, 'mutation site must be unique')
    mutated = source.replace(old, new, 1)
    try:
        path.write_text(mutated)
        code, output = run(target, test, os.environ.copy())
    finally:
        path.write_bytes(original)
    restored, restored_output = run(target, test, os.environ.copy())
    report = {'gate_sha': SHA, 'control': name, 'input': relative,
              'original_sha256': hashlib.sha256(original).hexdigest(),
              'mutant_sha256': hashlib.sha256(mutated.encode()).hexdigest(),
              'test': test, 'named_assertion': assertion, 'mutant_exit': code,
              'assertion_seen': assertion in output, 'restored_exit': restored,
              'restored_input_identical': path.read_bytes() == original}
    (OUT / f'{name}.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)
    if code != 101 or assertion not in output or restored != 0:
        print(output[-7000:]);print(restored_output[-7000:])
        sys.exit('invalid or surviving mutation; finding retained in report')

for name, target, test, assertion in [
    ('dispatch','public_classes','public_escaped_surrogate_callback_loss_is_inventoried',
     'actual source escaped surrogate invokes dispatch callback once'),
    ('alias','public_classes','public_escaped_surrogate_callback_loss_is_inventoried',
     'actual source same-generation alias receives second price 12.3 rather than 4.2'),
    ('private-open','private_classes','private_escaped_surrogate_order_callback_loss_is_inventoried',
     'actual synchronous onOpen preserves original thrown failure'),
    ('exchange-client','private_classes','exchange_owners_bind_real_children_clients_and_native_state_before_source',
     'actual PM FAPI client remains primary-only FAPI factory'),
]:
    env = os.environ.copy();env['SAFE_CEX_CLASS_SOURCE_CONTROL'] = name
    code, output = run(target, test, env)
    restored, restored_output = run(target, test, os.environ.copy())
    report = {'gate_sha':SHA,'control':f'source-{name}','mutation':'actual TypeScript body in isolated VM memory',
              'test':test,'named_assertion':assertion,'mutant_exit':code,
              'assertion_seen':assertion in output,'restored_exit':restored}
    (OUT / f'source-{name}.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report),flush=True)
    if code != 101 or assertion not in output or restored != 0:
        print(output[-7000:]);print(restored_output[-7000:])
        sys.exit('invalid or surviving source control; finding retained in report')
print('PASS: five Rust family mutations and four actual-source corruptions; every restore passed',flush=True)
