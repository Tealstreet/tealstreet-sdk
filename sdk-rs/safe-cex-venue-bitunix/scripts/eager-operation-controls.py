#!/usr/bin/env python3
"""Compile independent faults in the three real native async boundaries."""
import os
from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
crate = sdk / 'safe-cex-venue-bitunix'
out = sdk / 'target/bitunix-eager-controls'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=str(sdk.parent / 'node_modules/typescript'))
base = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
        '-p', 'safe-cex-venue-bitunix', '--lib']

def run(label, test, failure=None):
    log = out / (label + '.log')
    with log.open('w') as stream:
        result = subprocess.run(base + [test, '--', '--test-threads=1'], cwd=sdk,
                                env=env, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text()
    if failure:
        assert result.returncode == 101 and 'panicked at' in text and failure in text, text[-6000:]
        assert 'error[E' not in text and 'could not compile' not in text, text[-6000:]
    else:
        assert result.returncode == 0 and '0 failed' in text, text[-6000:]
    print(label + ': ' + ('compiled RED' if failure else 'GREEN'), flush=True)

controls = [
    ('exchange', ['adapter_rest.rs', 'adapter_orders.rs', 'adapter_history.rs'],
     'safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {',
     'native_query_tests::discarded', 'discarded native read'),
    ('http', ['native_http.rs'],
     'safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {',
     'native_query_tests::ready_native_http', 'ready HTTP must retain source await fence'),
    ('login', ['sockets.rs'],
     'safe_cex_exchange::eager::start_eager(&runtime, async move {',
     'native_query_tests::private_open', 'private onOpen must begin source sendLogin sync'),
]
for name, files, marker, test, assertion in controls:
    originals = {crate / 'src' / file: (crate / 'src' / file).read_bytes() for file in files}
    try:
        for file, original in originals.items():
            text = original.decode()
            assert marker in text
            text = text.replace(marker, 'Box::pin(async move {')
            if name == 'login':
                text = text.replace('        let runtime: Rc<dyn ExchangeRuntime> = self.runtime();\n', '')
                text = text.replace('ExchangeFuture, ExchangeRuntime,', 'ExchangeFuture,')
            file.write_text(text)
        run(name + '-fault', test, assertion)
    finally:
        for file, original in originals.items():
            file.write_bytes(original)
    run(name + '-restore', test)
