#!/usr/bin/env python3
"""Remove only the DXtrade registry entry; require a compiling E2E failure."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-registry/src/rust_venues.rs'
TEST = 'breakoutprop_registry_logs_in_reads_places_and_cancels_over_native_http'
CMD = ['cargo', '+1.99.0', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-registry', '--test', 'native_host_breakoutprop', '--jobs', '1', TEST, '--', '--exact']
original = SOURCE.read_bytes()
logs = Path(tempfile.mkdtemp(prefix='breakout-registry-control-'))
def run(label):
    result = subprocess.run(CMD, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (logs / (label + '.log')).write_text(result.stdout)
    print(label, result.returncode, flush=True)
    return result
print('logs:', logs, flush=True)
baseline = run('baseline')
assert baseline.returncode == 0 and '1 passed; 0 failed' in baseline.stdout, baseline.stdout
try:
    entry = b'    (ExchangeName::Breakoutprop, breakoutprop_constructor),\n'
    assert original.count(entry) == 1
    SOURCE.write_bytes(original.replace(entry, b''))
    failed = run('missing_entry')
    assert failed.returncode == 101 and 'error[E' not in failed.stdout, failed.stdout
    assert f'test {TEST} ... FAILED' in failed.stdout, failed.stdout
finally:
    SOURCE.write_bytes(original)
restored = run('restored')
assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, restored.stdout
assert SOURCE.read_bytes() == original
print('1/1 compiling registry control failed its named native E2E; restored 1/1 pass', flush=True)
