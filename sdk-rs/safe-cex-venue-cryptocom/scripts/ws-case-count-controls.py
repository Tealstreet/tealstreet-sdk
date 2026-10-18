"""Compiled truncated-walk controls for independently fixed native fixture counts."""
from pathlib import Path
import subprocess
SDK = Path(__file__).resolve().parents[2]
CRATE = SDK / 'safe-cex-venue-cryptocom'
CONTROLS = [
    ('OrderData', CRATE / 'tests/accepted_ws_record_shapes.rs', b'fn ws_record_orderdata()', ['--test', 'accepted_ws_record_shapes'], 'ws_record_orderdata', '76', 'CryptocomWsOrderData executed fixture count'),
    ('field', CRATE / 'src/ws_payloads.rs', b'mod accepted_ws_field {', ['--lib'], 'ws_payloads::accepted_ws_field::field_native_contract', '19', 'ws field executed fixture count'),
]
def run(target, test):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p', 'safe-cex-venue-cryptocom', *target, test, '--', '--exact'], cwd=SDK, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr
for name, file, start_marker, target, test, retained, assertion in CONTROLS:
    code, output = run(target, test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output
    original = file.read_bytes()
    start = original.index(start_marker)
    end = original.index(b'\n}\n', start) + 3
    region = original[start:end]
    before = b'for case in cases {'
    after = ('for case in cases.into_iter().take('+retained+') {').encode()
    assert region.count(before) == 1, name
    try:
        file.write_bytes(original[:start] + region.replace(before, after) + original[end:])
        code, output = run(target, test)
        assert code == 101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output, output
        assert assertion in output, output
    finally:
        file.write_bytes(original)
    assert file.read_bytes() == original
    code, output = run(target, test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output
    print(f'{name} fixture walk: compiled named count failure (exit 101); exact restoration PASS', flush=True)
