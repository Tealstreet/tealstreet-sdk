#!/usr/bin/env python3
"""Prove the native BingX HTTP factory retains each source factory behavior."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-bingx/src/binding/http.rs'
original = source.read_text()
marker = '        client\n            .get("interceptors")'
assert original.count(marker) == 1
cases = [
    ('timeout', 'factory_timeout_is_independent_of_recv_window',
     original.replace(marker, '        client.get("defaults").and_then(|d| d.define("timeout", Js::Number(defaults.timeout))).map_err(error)?;\n' + marker),
     'source factory fixed timeout'),
    ('Accept', 'factory_preserves_axios_accept_default',
     original.replace(marker, '        client.get("defaults").and_then(|d| d.get("headers")).map_err(error)?.delete(&safe_cex_exchange::js_runtime::value::utf16("Accept"));\n' + marker),
     'source factory Accept default'),
    ('adapter variant', 'factory_uses_default_adapter_for_initial_and_signature_resend',
     original.replace('RestAdapterVariant::Default,', 'RestAdapterVariant::BigInt,'),
     'left: BigInt'),
]

def run(test=None):
    command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-bingx', '--jobs', '1', '--test', 'binding']
    command += [test, '--', '--exact'] if test else ['factory_']
    return subprocess.run(command, cwd=workspace, text=True, capture_output=True)

try:
    baseline = run()
    assert baseline.returncode == 0 and '3 passed' in baseline.stdout, baseline.stdout + baseline.stderr
    for name, test, mutant, witness in cases:
        assert mutant != original
        source.write_text(mutant)
        result = run(test)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and f'test {test} ... FAILED' in output and witness in output, output
        print(f'CONTROL PASS: {name} compiled and failed its named E2E', flush=True)
        source.write_text(original)
        restored = run(test)
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout + restored.stderr
        print(f'RESTORED PASS: {name}', flush=True)
finally:
    source.write_text(original)
