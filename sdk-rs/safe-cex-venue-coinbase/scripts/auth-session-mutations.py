#!/usr/bin/env python3
"""Sequential compiling controls; each production mutation is restored in finally."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-auth-controls') / SHA[:10]
OUT.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'SAFE_CEX_TYPESCRIPT_PATH': '/Users/sam/cs/tealstreet-next/node_modules/typescript'}
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-coinbase', '--jobs', '2']
RESULTS = []

def run(label, binary, test, expected, diagnostic=None):
    command = CARGO + ['--test', binary, test, '--', '--exact']
    result = subprocess.run(command, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (OUT / (label + '.log')).write_text(result.stdout)
    record = {'label': label, 'code_sha': SHA, 'command': command, 'exit': result.returncode,
              'expected_exit': expected, 'diagnostic': diagnostic,
              'diagnostic_found': diagnostic is None or diagnostic in result.stdout,
              'compiling_test_failure': expected == 0 or ('test result: FAILED.' in result.stdout and 'error[E' not in result.stdout),
              'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff'])).hexdigest()}
    RESULTS.append(record)
    (OUT / 'results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    if result.returncode != expected or not record['diagnostic_found'] or not record['compiling_test_failure']:
        print(result.stdout[-10000:], flush=True)
        raise RuntimeError(label + ': unexpected result; a compiling survivor is a finding')

def control(label, file, old, new, binary, test, diagnostic):
    path = CRATE / 'src' / file
    original = path.read_bytes()
    source = original.decode()
    if source.count(old) != 1:
        raise RuntimeError(label + ': expected one mutation call site')
    try:
        path.write_text(source.replace(old, new, 1))
        run(label + '-mutated', binary, test, 101, diagnostic)
    finally:
        path.write_bytes(original)
    if path.read_bytes() != original:
        raise RuntimeError(label + ': restoration failed')
    run(label + '-restored', binary, test, 0)

control('auth-expiry', 'starbase_auth.rs', 'now < refresh_at', 'now <= refresh_at',
        'auth_session', 'auth_expiry_force_alias_mutation_headers_and_failures_match_source',
        'native auth result and retained extra fields')
control('await-boundary', 'starbase_auth.rs', 'if !self.queued {', 'if false {',
        'ws_controller', 'queued_disconnect_interleaves_with_ready_awaits_and_consumer_poll_is_passive',
        'consumer polling cannot execute activation producer')
control('disconnect-generation', 'starbase_ws_controller.rs',
        '.set(self.0.activation_generation.get() + 1.0);', '.set(self.0.activation_generation.get());',
        'ws_controller', 'controller_constructor_activation_policy_failure_and_ready_operations_match_source',
        'native controller state')
control('policy-scope', 'starbase_ws_controller.rs',
        'current.scope != expected_scope', '(current.scope != expected_scope && false)',
        'ws_controller', 'controller_constructor_activation_policy_failure_and_ready_operations_match_source',
        'native activation completion branch')
control('reaction-order', 'starbase_auth.rs', 'for reaction in reactions {',
        'let mut reactions = reactions;\n        reactions.sort_by_key(|reaction| matches!(reaction, PromiseReaction::Await(_)));\n        for reaction in reactions {',
        'auth_session', 'earlier_future_waiter_keeps_registration_order_with_auth_observer',
        'mixed Promise reactions retain registration order')
print('Five controls caught; all mutated production files restored byte for byte.', flush=True)
