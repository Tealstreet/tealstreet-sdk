"""Consume complete records, then corrupt each owned Vec before observation."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'safe-cex-venue-cryptocom/src/wire.rs'
SITES = [('CryptocomBalanceSummary', 'position_balances', False),
         ('CryptocomBookData', 'bids', False),
         ('CryptocomBookData', 'asks', False),
         ('CryptocomBook', 'data', False),
         ('CryptocomOrder', 'exec_inst', True)]

def run(test):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p',
        'safe-cex-venue-cryptocom', '--test', 'accepted_nested_record_shapes', test,
        '--', '--exact'], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr

def passing(test):
    code, output = run(test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output

for owner, field, optional in SITES:
    test = 'complex_record_' + owner.removeprefix('Cryptocom').lower()
    passing(test)
    for action in ['clear()', 'truncate(1)']:
        original = SOURCE.read_bytes()
        start = original.index(f"impl<'de> Deserialize<'de> for {owner} {{".encode())
        end = original.index(b'\n}\n', start)
        region = original[start:end]
        assert region.count(b'Ok(record)') == 1
        edit = (f'if let Some(values) = &mut record.{field} {{ values.{action}; }}'
                if optional else f'record.{field}.{action};')
        try:
            mutated = region.replace(b'Ok(record)', edit.encode() + b'\n                Ok(record)')
            SOURCE.write_bytes(original[:start] + mutated + original[end:])
            code, output = run(test)
            assert code == 101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output, output
            assert f'{owner} native fields' in output, output
        finally:
            SOURCE.write_bytes(original)
        assert SOURCE.read_bytes() == original
        passing(test)
        print(f'{owner}.{field}.{action}: fully consumed output failed native fields (exit 101); exact restoration PASS', flush=True)
