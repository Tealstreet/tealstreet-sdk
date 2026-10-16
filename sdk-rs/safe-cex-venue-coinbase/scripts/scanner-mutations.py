#!/usr/bin/env python3
"""Compiling, sequential controls for the new JSON key scanner slice; restore bytes even on failure."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-scanner-controls') / SHA[:10]
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
    path = CRATE / 'src/json_key_scanner.rs'
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

SCAN = 'grammar_utf16_duplicate_precedence_and_scanner_reuse_match_live_source'
control('duplicate-state', 'self.duplicate = true;', 'self.duplicate = false;',
        ['--test', 'json_key_scanner'], SCAN, 'decoded native key scan result')
control('depth-boundary', 'depth > 128', 'depth > 127',
        ['--test', 'json_key_scanner'], SCAN, 'decoded native key scan result')
control('decoded-key', '                value\n', '                value.wrapping_add(1)\n',
        ['--lib'], 'json_key_scanner::tests::decoded_key_units_preserve_surrogates_and_json_escapes',
        'decoded native JSON key code units')
print('Three compiling controls caught and restored byte for byte.', flush=True)
