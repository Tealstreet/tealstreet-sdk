#!/usr/bin/env python3
"""Compiled hosted-account omissions must fail for both KuCoin venue names."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
LOGS = Path(tempfile.mkdtemp(prefix='kucoin-hosted-controls-'))
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(SDK / 'Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-registry', '--test', 'native_host_kucoin_hosted']
TESTS = ['kucoin_futures_hosted_account_and_private_token_match_actual_source',
         'kucoin_spot_hosted_account_and_private_token_match_actual_source']


def run(name, expected, marker=None):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{name}.log').write_text(output)
    assert result.returncode == expected, f'{name}: exit {result.returncode}; {LOGS}'
    assert 'running 2 tests' in output and all(test in output for test in TESTS)
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output
        assert '0 passed; 2 failed' in output and 'panicked at' in output
        for label in ['kucoin', 'kucoinspot']:
            assert any(line.endswith(f'{marker} {label}') for line in output.splitlines()), f'{name}: missing {label} behavior; {LOGS}'
    else:
        assert '2 passed; 0 failed' in output
    print(f'{name}: exit {result.returncode}', flush=True)


host = SDK / 'safe-cex-registry/src/native_host.rs'
content = host.read_text()
start = content.index('    fn kucoin(\n')
end = content.index('    fn mexc(', start)
region = content[start:end]
old = '                    remote.clone(),'
assert region.count(old) == 1
mutated_host = content[:start] + region.replace(old, '                    remote.clone().filter(|_| false),') + content[end:]
controls = [
    ('http-signer', host, content, mutated_host,
     'KUCOIN HOSTED account RPC reaches shared signer'),
    ('typed-capability', SDK / 'safe-cex-venue-kucoin/src/binding/http.rs',
     '    fn hosted_signer(&self) -> Option<HostedSigner> {\n        self.hosted_signer.clone()\n    }',
     '    fn hosted_signer(&self) -> Option<HostedSigner> {\n        None\n    }',
     'KUCOIN HOSTED typed capability'),
    ('source-api-bypass', SDK / 'safe-cex-venue-kucoin/src/api.rs',
     '    let remote = http.signer.is_some();',
     '    let remote = http.signer.is_some() && false;',
     'KUCOIN HOSTED skips local empty-secret signing'),
]
for name, source, before, after, marker in controls:
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, name
    run(f'{name}-baseline', 0)
    try:
        source.write_text(text.replace(before, after))
        run(name, 101, marker)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{name}-restored', 0)
print(f'All three compiled controls fail for both names; exact restorations pass; logs {LOGS}')
