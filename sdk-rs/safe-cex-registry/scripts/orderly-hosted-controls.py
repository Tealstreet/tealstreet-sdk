#!/usr/bin/env python3
"""Compile hosted bridge omissions, a secret fallback and wire signature loss."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
LOGS = Path(tempfile.mkdtemp(prefix='orderly-hosted-controls-'))
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(SDK / 'Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-registry', '--test', 'native_host_orderly_hosted']
TESTS = ['orderly_hosted_http_with_local_secret_and_local_only_ws_match_source',
         'orderly_hosted_empty_secret_preserves_source_http_and_ws_failures']


def run(name, failures, markers=()):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{name}.log').write_text(output)
    assert result.returncode == (101 if failures else 0), f'{name}: exit {result.returncode}; {LOGS}'
    assert 'Finished `test`' in output and 'error[E' not in output
    assert 'running 2 tests' in output and all(test in output for test in TESTS)
    assert f'{2-failures} passed; {failures} failed' in output, f'{name}: census; {LOGS}'
    for marker in markers:
        assert marker in output, f'{name}: absent behavioral assertion {marker}; {LOGS}'
    print(f'{name}: exit {result.returncode}; {failures} expected failures', flush=True)


host = SDK / 'safe-cex-registry/src/native_host.rs'
original_host = host.read_text()
start = original_host.index('    fn orderly(')
end = original_host.index('    #[expect', start)
region = original_host[start:end]
needle = '                        remote.clone(),'
assert region.count(needle) == 1
wire_needle = '''impl RestAdapter for SourceApiRest {
    fn execute(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move {'''
assert original_host.count(wire_needle) == 1
controls = [
    ('hosted-capability', host, original_host,
     original_host[:start] + region.replace(needle, '                        remote.clone().filter(|_| false),') + original_host[end:],
     2, ['ORDERLY HOSTED private HTTP reaches shared signer', 'ORDERLY HOSTED typed capability']),
    ('account-name', host, original_host,
     original_host[:start] + region.replace(needle, '                        remote.clone().map(|(signer, _)| (signer, None)),') + original_host[end:],
     2, ['ORDERLY HOSTED metadata accountName']),
    ('invented-secret-fallback', SDK / 'safe-cex-venue-orderly/src/encryption.rs',
     "secret.split(':').nth(1).ok_or_else",
     "secret.split(':').nth(1).or(Some(\"11111111111111111111111111111111\")).ok_or_else",
     1, ['ORDERLY HOSTED private HTTP still requires source local secret']),
    ('wire-signature', host, wire_needle,
     wire_needle.replace('        Box::pin(async move {',
                         '        let _ = request.headers.shift_remove("orderly-signature");\n        Box::pin(async move {'),
     1, ['ORDERLY HOSTED local signing survives callback to wire orderly-signature']),
]
start_control = int(os.environ.get('ORDERLY_HOSTED_CONTROL_START', '0'))
assert 0 <= start_control < len(controls)
for name, source, before, after, failures, markers in controls[start_control:]:
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, name
    run(f'{name}-baseline', 0)
    try:
        source.write_text(text.replace(before, after))
        run(name, failures, markers)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{name}-restored', 0)
print(f'{len(controls)-start_control} compiled faults caught; exact restorations pass all six-name cases; logs {LOGS}')
