#!/usr/bin/env python3
"""Compiling, sequential controls for the new frame classification slice; restore bytes even on failure."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-frame-controls') / SHA[:10]
OUT.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'SAFE_CEX_TYPESCRIPT_PATH': '/Users/sam/cs/tealstreet-next/node_modules/typescript'}
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-coinbase', '--jobs', '2']
RESULTS = []

def run(label, selector, test, expected, diagnostic=None):
    command = CARGO + selector + [test, '--', '--exact']
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
        raise RuntimeError(label + ': unexpected gate result; compiling survivors are findings')

def control(label, old, new, selector, test, diagnostic):
    path = CRATE / 'src/starbase_public_frames.rs'
    original = path.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, label + ': expected one mutation call site'
    try:
        path.write_text(source.replace(old, new, 1))
        run(label + '-mutated', selector, test, 101, diagnostic)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    run(label + '-restored', selector, test, 0)

MATCH = 'native_notification_response_and_raw_frame_classification_match_live_source'
SELECT = ['--test', 'frame_classification']
control('desired-membership', '!desired.contains(&channel)', '(false && !desired.contains(&channel))',
        SELECT, MATCH, 'decoded native classification kind')
control('timing-difference', 'end - start != diff', 'end - start == diff',
        SELECT, MATCH, 'decoded native classification kind')
control('duplicate-precedence', 'return Ok(unknown(UnknownReason::DuplicateJsonKey))',
        'return Ok(unknown(UnknownReason::InvalidJson))', SELECT, MATCH, 'decoded native unknown reason')
control('payload-alias', 'data: params.get("data").expect("own data"),',
        'data: match params.get("data").expect("own data") { FrameValue::Object(value) => FrameValue::Object(Rc::new(RefCell::new(value.borrow().clone()))), other => other },',
        SELECT, 'native_payload_aliases_array_aliases_and_primitive_copies_match_source', 'native payload object alias')
print('Four compiling controls caught; production restored byte for byte.', flush=True)
