#!/usr/bin/env python3
"""Compiled regressions for the native hosted-account capability and cancellation edges."""
from pathlib import Path
import subprocess
import sys

workspace = Path(__file__).resolve().parents[2]
native = workspace / 'safe-cex-registry/src/native_host.rs'
exchange = workspace / 'safe-cex-venue-apexomni/src/binding/exchange.rs'
sockets = workspace / 'safe-cex-venue-apexomni/src/binding/sockets.rs'
original = {p: p.read_text() for p in [native, exchange, sockets]}
a = original[native].index('    fn apexomni(&self)')
b = original[native].index('    fn bingx(&self)', a)
chunk = original[native][a:b]
assert chunk.count('self.remote.clone()') == 1
account_start = original[sockets].index('    fn remote_signer_account(&self)')
account_end = original[sockets].index('    fn remote_sign(&self', account_start)
open_start = original[sockets].index('                    let mut future = callback();')
open_end = original[sockets].index('\n                }),', open_start)
unowned = '''                    let future = callback();
                    Ok(Js::Promise(Promise::spawn(
                        &runtime,
                        Box::pin(async move {
                            future.await.map_err(Thrown::from)?;
                            Ok(Js::Undefined)
                        }),
                    )))'''
hosted = 'apexomni_native_hosted_signer_authenticates_http_and_ws'
disposal = 'apexomni_native_dispose_cancels_pending_hosted_ws_auth'
cases = [
    ('HTTP capability', native, original[native][:a] + chunk.replace('self.remote.clone()', 'None') + original[native][b:], hosted, 'hosted signer participates in BaseExchange canSign'),
    ('Base capability', exchange, original[exchange].replace('base.signers.remote_signer = remote.map(|(signer, _)| signer);', 'base.signers.remote_signer = None;'), hosted, 'hosted signer participates in BaseExchange canSign'),
    ('WS capability', sockets, original[sockets][:account_start] + '    fn remote_signer_account(&self) -> Option<J> { None }\n' + original[sockets][account_end:], hosted, 'timed out waiting for Apex requests remote WS auth'),
    ('owned WS continuation', sockets, (original[sockets][:open_start] + unowned + original[sockets][open_end:]).replace('    task::{Context, Poll, Waker},\n', ''), disposal, 'timed out waiting for hosted WS signing future is canceled'),
]
def run(test):
    return subprocess.run(['cargo', 'test', '--locked', '-p', 'safe-cex-registry', '--jobs', '1', '--test', 'native_host_apexomni', test, '--', '--exact'], cwd=workspace, capture_output=True, text=True, timeout=600)
selected = set(sys.argv[1:])
assert selected <= {case[0] for case in cases}, selected
try:
    for name, path, mutant, test, witness in cases:
        if selected and name not in selected:
            continue
        assert mutant != original[path]
        path.write_text(mutant)
        result = run(test)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and f'test {test} ... FAILED' in output and witness in output, output
        print(f'CONTROL PASS: {name} compiled and failed its named native E2E', flush=True)
        path.write_text(original[path])
        restored = run(test)
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout + restored.stderr
        print(f'RESTORED PASS: {name}', flush=True)
finally:
    for path, source in original.items():
        path.write_text(source)
