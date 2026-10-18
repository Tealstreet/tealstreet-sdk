"""Prove the native scaffold witness rejects a compiling wrong registry factory."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'safe-cex-registry/src/rust_venues.rs'
TEST = 'txflow_native_registry_preserves_phase_zero_refusal_without_io'


def run():
    result = subprocess.run([
        'cargo', 'test', '--locked', '-q', '--jobs', '1', '-p', 'safe-cex-registry',
        '--test', 'native_host_txflow', TEST, '--', '--exact',
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


code, output = run()
assert code == 0 and 'test result: ok. 1 passed' in output, output
original = SOURCE.read_bytes()
old = b'(ExchangeName::Txflow, txflow_constructor)'
assert original.count(old) == 1
try:
    SOURCE.write_bytes(original.replace(old, b'(ExchangeName::Txflow, backpack_constructor)'))
    code, output = run()
    assert code == 101 and TEST in output and 'test result: FAILED' in output, output
finally:
    SOURCE.write_bytes(original)
code, output = run()
assert code == 0 and 'test result: ok. 1 passed' in output, output
print('TxFlow registry: compiling factory swap fails101; exact restoration passes0')
