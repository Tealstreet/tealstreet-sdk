#!/usr/bin/env python3
"""Compiled Aster native-host regressions; exact production-byte restoration."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
HOST = SDK / 'safe-cex-registry/src/native_host.rs'
VENUE = SDK / 'safe-cex-venue-asterdex/src'
ACCOUNT = 'asterdex_hosted_account_preserves_source_credentials_and_signed_wire'
CLOCK = 'asterdex_hosted_public_start_keeps_bare_clock_unsigned'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-hosted-controls-'))
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(SDK / 'Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-registry', '--test', 'native_host_asterdex']

def run(name, test, expected, marker=None):
    result = subprocess.run([*COMMAND, test, '--', '--exact'], cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{name}.log').write_text(output)
    assert result.returncode == expected, f'{name}: exit {result.returncode}; {LOGS}'
    assert '1 test' in output and '0 tests' not in output
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output
        assert test in output and 'panicked at' in output
        assert marker in output, f'{name}: missing behavioral assertion {marker}; {LOGS}'
    print(f'{name}: exit {result.returncode}', flush=True)

host = HOST.read_text()
start = host.index('    fn asterdex(')
end = host.index('    fn hosted_signer', start)
region = host[start:end]
old = '                        remote.clone(),'
assert region.count(old) == 1
mutated_host = host[:start] + region.replace(old, '                        None,') + host[end:]
controls = [
    ('hosted-capability', HOST, host, mutated_host, ACCOUNT, 'ASTER HOSTED typed canSign observes signer'),
    ('typed-presence', VENUE / 'native_exchange.rs', None,
     ('self.base().signers.remote_signer.is_some()', 'false'), ACCOUNT, 'ASTER HOSTED typed canSign observes signer'),
    ('validation-secret', VENUE / 'order_execution.rs', None,
     (' && self.owner.signers.remote_signer.is_none()', ''), ACCOUNT, 'ASTER HOSTED validateAccount uses signer without local secret'),
    ('bare-clock', VENUE / 'binding/globals.rs', None,
     ('            ("safeCexSkipRemoteSigning", Js::Bool(true)),', ''), CLOCK, 'ASTER HOSTED bare clock bypasses signer'),
]
for name, source, original_text, mutation, test, marker in controls:
    original = source.read_bytes()
    text = original.decode()
    if original_text is not None:
        assert text == original_text
        changed = mutation
    else:
        before, after = mutation
        assert text.count(before) == 1, name
        changed = text.replace(before, after)
    run(f'{name}-baseline', test, 0)
    try:
        source.write_text(changed)
        run(name, test, 101, marker)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{name}-restored', test, 0)
print(f'All four compiled controls fail; exact restorations pass; logs {LOGS}')
