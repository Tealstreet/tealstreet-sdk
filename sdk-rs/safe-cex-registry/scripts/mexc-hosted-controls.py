#!/usr/bin/env python3
"""Compile each omitted MEXC capability, require named failures, restore/pass."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
LOGS = Path(tempfile.mkdtemp(prefix='mexc-hosted-controls-'))
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(SDK / 'Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-registry', '--test', 'native_host_mexc_hosted']
TESTS = ['mexc_hosted_http_and_ws_match_actual_source',
         'mexc_hosted_ws_error_preserves_source_log_without_login',
         'mexc_hosted_ws_missing_signature_preserves_source_log_without_login']


def run(name, expected, marker=None):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{name}.log').write_text(output)
    assert result.returncode == expected, f'{name}: exit {result.returncode}; {LOGS}'
    assert 'running 3 tests' in output and all(test in output for test in TESTS)
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output
        assert '0 passed; 3 failed' in output and 'panicked at' in output
        assert output.count(marker) >= 3, f'{name}: missing named behaviors; {LOGS}'
    else:
        assert '3 passed; 0 failed' in output
    print(f'{name}: exit {result.returncode}', flush=True)


host = SDK / 'safe-cex-registry/src/native_host.rs'
constructors = SDK / 'safe-cex-registry/src/rust_venues.rs'
controls = [
    ('http-signer', host, '    fn mexc(', '    fn ',
     '                self.remote.clone(),',
     '                self.remote.clone().filter(|_| false),',
     'MEXC HOSTED private HTTP reaches shared signer'),
    ('typed-capability', constructors, 'pub fn mexc_constructor(', 'pub fn asterdex_constructor(',
     '        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);',
     '        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer).filter(|_| false);',
     'MEXC HOSTED typed capability'),
    ('ws-account-name', constructors, 'pub fn mexc_constructor(', 'pub fn asterdex_constructor(',
     '                remote_account_name: host.hosted_signer().and_then(|(_, account)| account),',
     '                remote_account_name: None,',
     'MEXC HOSTED WS metadata accountName'),
]
for name, source, first, last, before, after, marker in controls:
    original = source.read_bytes()
    text = original.decode()
    start = text.index(first)
    end = text.index(last, start + len(first))
    region = text[start:end]
    assert region.count(before) == 1, name
    run(f'{name}-baseline', 0)
    try:
        source.write_text(text[:start] + region.replace(before, after) + text[end:])
        run(name, 101, marker)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{name}-restored', 0)
print(f'All three compiled controls fail all outcomes; exact restorations pass; logs {LOGS}')
