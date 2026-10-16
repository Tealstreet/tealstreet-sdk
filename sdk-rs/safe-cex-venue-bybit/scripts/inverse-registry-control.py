#!/usr/bin/env python3
"""Remove only the Inverse Rust registry entry, then require native refusal."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-registry/src/rust_venues.rs'
ORIGINAL = SOURCE.read_bytes()
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-registry', '--test', 'native_host_bybit']
LOGS = Path(os.environ.get('INVERSE_REGISTRY_LOG_DIR') or tempfile.mkdtemp(prefix='bybit-inverse-registry-control-'))
LOGS.mkdir(parents=True, exist_ok=True)
print(f"Control logs: {LOGS}", flush=True)

def run(label, negative=False):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    if negative:
        assert result.returncode == 101 and 'Finished `test`' in output and 'error[E' not in output, f'{label}: compiled failure required; {LOGS}'
        for name in ('bybit_and_demo_construct_and_sign_through_native_registry', 'bybit_and_demo_use_hosted_signer_without_local_credentials'):
            assert f'test {name} ... FAILED' in output, f'{label}: native constructor proof required: {name}; {LOGS}'
    else:
        assert result.returncode == 0 and '3 passed; 0 failed' in output, f'{label}: all three native host tests required; {LOGS}'
    print(f'{label}: exit {result.returncode}', flush=True)

try:
    run('baseline')
    needle = '    (ExchangeName::Bybitinverse, bybit_constructor),\n'
    text = ORIGINAL.decode()
    assert text.count(needle) == 1
    SOURCE.write_text(text.replace(needle, ''))
    try:
        run('entry-removed', True)
    finally:
        SOURCE.write_bytes(ORIGINAL)
    run('restored')
finally:
    SOURCE.write_bytes(ORIGINAL)
assert SOURCE.read_bytes() == ORIGINAL
print(f'Compiled entry-removal control caught; exact restoration passes; logs {LOGS}')
