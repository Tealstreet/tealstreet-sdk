#!/usr/bin/env python3
"""Compiled Backpack hosted HTTP/source WS regressions; exact byte restoration."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
VENUE = SDK / 'safe-cex-venue-backpack/src'
HTTP = 'backpack_registry_hosted_http_matches_actual_source_and_public_bypass'
WS = 'backpack_secretless_private_auth_preserves_actual_source_failure_and_log'
LOGS = Path(tempfile.mkdtemp(prefix='backpack-hosted-controls-'))
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(SDK / 'Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-registry', '--test', 'native_host_backpack_hosted']


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


adapter = VENUE / 'private_adapter.rs'
original_adapter = adapter.read_text()
start = original_adapter.index('                    response.await.map(|s| units(&s)).map_err(|message| {')
end = original_adapter.index('\n                })', start)
old_adapter = original_adapter[start:end]
controls = [
    ('http-signer', VENUE / 'native_host.rs',
     'signer: remote.as_ref().map(', 'signer: remote.as_ref().filter(|_| false).map(',
     HTTP, 'BACKPACK HOSTED private RPC only'),
    ('typed-presence', SDK / 'safe-cex-registry/src/rust_venues.rs',
     '        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);',
     '        base.signers.remote_signer = None;', HTTP, 'BACKPACK HOSTED capability reaches typed base'),
    ('public-bypass', VENUE / 'api_factory.rs',
     '            next.skip_remote_signing = true;', '            next.skip_remote_signing = false;',
     HTTP, 'BACKPACK HOSTED private RPC only'),
    ('source-error', adapter, old_adapter,
     '                    response.await.map(|s| units(&s)).map_err(ExchangeError::new)',
     WS, 'BACKPACK HOSTED source error identity and log authenticate'),
]
for name, source, before, after, test, marker in controls:
    original = source.read_bytes()
    content = original.decode()
    # The capability assignment also appears in other constructors; restrict the
    # mutation to the concrete Backpack constructor and leave their bodies intact.
    if name == 'typed-presence':
        start = content.index('pub fn backpack_constructor(')
        end = content.index('\npub fn ', start + 1)
        region = content[start:end]
        assert region.count(before) == 1, name
        changed = content[:start] + region.replace(before, after) + content[end:]
    else:
        assert content.count(before) == 1, name
        changed = content.replace(before, after)
    run(f'{name}-baseline', test, 0)
    try:
        source.write_text(changed)
        run(name, test, 101, marker)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{name}-restored', test, 0)
print(f'All four compiled controls fail; exact restorations pass; logs {LOGS}')
