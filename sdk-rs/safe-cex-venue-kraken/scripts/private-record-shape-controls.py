#!/usr/bin/env python3
"""Remove only each object guard; full valid arrays must fail native assertions."""
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-kraken'
controls = [
    ('futures_candles.rs', 'FuturesOptionsWire',
     'futures_candles::accepted_shape::futures_options_rejects_complete_positional_array',
     '["PF_XRPUSD","1m",17.25,31.75,123.5]'),
    ('spot_reads.rs', 'SpotOptionsWire',
     'spot_reads::accepted_shape::spot_options_rejects_complete_positional_array',
     '["XBT/USD","15m",47.25]'),
    ('reads.rs', 'AccountLogParamsWire',
     'reads::accepted_shape::account_log_params_rejects_complete_positional_array',
     '[19.5,7.25]'),
    ('reads.rs', 'HistoricalOptionsWire',
     'reads::accepted_shape::historical_options_rejects_complete_positional_array',
     '["account-A",71.25,93.5]'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name, _, _, _ in controls}
for name, wire, _, _ in controls:
    anchor = f'{wire}::deserialize(safe_cex_types::strict_decode::object(decoder))'.encode()
    assert originals[name].count(anchor) == 1, (name, wire, 'unique guard required')


def run(test='accepted_shape', exact=False):
    args = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-kraken', '--lib', test, '--jobs', '1', '--']
    if exact:
        args.append('--exact')
    args.append('--nocapture')
    result = subprocess.run(args, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'},
                            capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


baseline, output = run()
assert baseline.returncode == 0 and '4 passed; 0 failed; 0 ignored' in output, output[-10000:]
print('BASELINE PASS: four native owner tests, seven routes, zero ignored', flush=True)
try:
    for name, wire, test, array in controls:
        source = crate / 'src' / name
        original = originals[name]
        old = f'{wire}::deserialize(safe_cex_types::strict_decode::object(decoder))'.encode()
        new = f'{wire}::deserialize(decoder)'.encode()
        try:
            source.write_bytes(original.replace(old, new))
            changed, output = run(test, exact=True)
            assert (changed.returncode not in (0, 126)
                    and f'test {test} ... FAILED' in output and 'test result: FAILED' in output
                    and f'input={array}' in output
                    and 'complete valid positional array or invalid record accepted' in output
                    and 'could not compile' not in output), output[-10000:]
            print(f'CONTROL PASS: {wire}; compiled named complete-array admission failure', flush=True)
        finally:
            for file, contents in originals.items():
                (crate / 'src' / file).write_bytes(contents)
        restored, output = run()
        assert restored.returncode == 0 and '4 passed; 0 failed; 0 ignored' in output, output[-10000:]
        print(f'RESTORED PASS: {wire}; all four native owner tests', flush=True)
finally:
    for name, contents in originals.items():
        (crate / 'src' / name).write_bytes(contents)
assert all((crate / 'src' / name).read_bytes() == contents for name, contents in originals.items())
print('FINAL PASS: exact guarded sources restored; four native tests, zero ignored', flush=True)
