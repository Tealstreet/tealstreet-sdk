"""Native complete-array proof by bypassing both concrete object admission stages.

The captured object boundary and private remote object guard both reject arrays.
Removing only the inner guard cannot widen the concrete decoder. Each mutation
therefore delegates the concrete owner directly to its same original companion,
then requires the complete-array assertion to fail at runtime, not compilation.
"""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'safe-cex-venue-cryptocom/src/wire.rs'
OWNERS = ['CryptocomInstrument', 'CryptocomTicker', 'CryptocomCandlestick',
          'CryptocomPositionBalance', 'CryptocomError', 'CryptocomAccountIdentity']

def run(test):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p',
        'safe-cex-venue-cryptocom', '--test', 'accepted_scalar_record_shapes', test,
        '--', '--exact'], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr

def passing(test):
    code, output = run(test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output

for owner in OWNERS:
    test = 'complete_array_' + owner.removeprefix('Cryptocom').lower()
    passing(test)
    original = SOURCE.read_bytes()
    impl_start = original.index(f"impl<'de> Deserialize<'de> for {owner} {{".encode())
    start = original.index(b'{\n', original.index(b'fn deserialize', impl_start)) + 2
    # Find the exact end of this impl using the following top-level declaration.
    end_impl = original.index(b'\n}\n', start)
    body_end = original.rindex(b'\n    }', start, end_impl)
    body = original[start:body_end]
    assert b'crate::wire_record::capture(' in body and b'strict_decode::object(' in body
    assert f'Wire{owner}::deserialize'.encode() in body
    try:
        SOURCE.write_bytes(original[:start] + f'        Wire{owner}::deserialize(deserializer)'.encode() + original[body_end:])
        code, output = run(test)
        assert code == 101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output, output
        assert f'complete positional array accepted for {owner}' in output, output
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    passing(test)
    print(f'{owner}: both object stages bypassed, complete-array assertion failed101; exact restoration PASS', flush=True)
