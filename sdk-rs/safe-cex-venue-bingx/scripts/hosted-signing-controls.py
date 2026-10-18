#!/usr/bin/env python3
"""Prove each capability edge is needed by the real native hosted-account E2E."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
files = [workspace / name for name in [
    'safe-cex-registry/src/native_host.rs',
    'safe-cex-venue-bingx/src/exchange.rs',
    'safe-cex-venue-bingx/src/binding/http.rs',
]]
original = {p: p.read_text() for p in files}
native = files[0]
a = original[native].index('    fn bingx(&self)')
b = original[native].index('    fn coinbase(', a)
chunk = original[native][a:b]
assert chunk.count('self.remote.clone()') == 1
cases = [
    ('HTTP capability', native, original[native][:a] + chunk.replace('self.remote.clone()', 'None') + original[native][b:], 'hosted signer participates in BaseExchange canSign'),
    ('Base capability', files[1], original[files[1]].replace('base.signers = hosts.exchange.signers();', 'base.signers = Default::default();'), 'hosted signer participates in BaseExchange canSign'),
    ('local signing bypass', files[2], original[files[2]].replace('self.scope.http.hosted_signer().is_some()', 'false'), 'hosted capability bypasses local API signing'),
]
test = 'bingx_native_hosted_signer_keeps_secret_remote'
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-registry', '--jobs', '1', '--test', 'native_host_bingx', test, '--', '--exact']
def run():
    return subprocess.run(command, cwd=workspace, text=True, capture_output=True, timeout=180)
try:
    baseline = run()
    assert baseline.returncode == 0 and '1 passed' in baseline.stdout, baseline.stdout + baseline.stderr
    for name, path, mutant, witness in cases:
        assert mutant != original[path]
        path.write_text(mutant)
        result = run()
        output = result.stdout + result.stderr
        assert result.returncode == 101 and f'test {test} ... FAILED' in output and witness in output, output
        print(f'CONTROL PASS: {name} compiled and failed the named native E2E', flush=True)
        path.write_text(original[path])
        restored = run()
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout + restored.stderr
        print(f'RESTORED PASS: {name}', flush=True)
finally:
    for path, saved in original.items():
        path.write_text(saved)
