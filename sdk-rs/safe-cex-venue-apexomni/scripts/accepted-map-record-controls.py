#!/usr/bin/env python3
"""Isolate each reviewed object's guard removal, then restore exact source bytes."""
import json
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-apexomni'
fixtures = json.loads((crate / 'tests/support/accepted_map_record_fixtures.json').read_text())
assert len(fixtures) == 26, 'fixed independently reviewed owner census'
originals = {row['module']: (crate / 'src' / (row['module'] + '.rs')).read_bytes() for row in fixtures.values()}
for name, row in fixtures.items():
    anchor = f'{name}Wire::deserialize(safe_cex_types::strict_decode::object(decoder))'.encode()
    assert originals[row['module']].count(anchor) == 1, (name, 'unique reviewed guard required')


def restore():
    for module, contents in originals.items():
        (crate / 'src' / (module + '.rs')).write_bytes(contents)


def run(test=None):
    args = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-apexomni', '--test', 'accepted_map_records', '--jobs', '1']
    if test:
        args += [test, '--', '--exact', '--nocapture']
    else:
        args += ['--', '--nocapture']
    result = subprocess.run(args, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'},
                            capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


def restored_pass(label):
    result, output = run()
    assert result.returncode == 0 and '26 passed; 0 failed; 0 ignored' in output, output[-16000:]
    assert all((crate / 'src' / (module + '.rs')).read_bytes() == data for module, data in originals.items())
    print(f'{label}: all26 native witnesses PASS, exact source bytes', flush=True)


restored_pass('BASELINE')
try:
    for name, row in fixtures.items():
        module = row['module']
        source = crate / 'src' / (module + '.rs')
        original = originals[module]
        old = f'{name}Wire::deserialize(safe_cex_types::strict_decode::object(decoder))'.encode()
        new = f'{name}Wire::deserialize(decoder)'.encode()
        test = name + '_rejects_complete_positional_array'
        try:
            source.write_bytes(original.replace(old, new))
            result, output = run(test)
            assert (result.returncode == 101
                    and f'test {test} ... FAILED' in output and 'test result: FAILED' in output
                    and 'case="complete valid positional array"' in output
                    and f'input={row["array"]}' in output
                    and 'complete valid positional array or invalid record accepted' in output
                    and 'could not compile' not in output), output[-16000:]
            print(f'CONTROL PASS: {name}; isolated compiled named COMPLETE VALID array admission failure', flush=True)
        finally:
            restore()
        restored_pass('RESTORED ' + name)
finally:
    restore()
restored_pass('FINAL')
print('ALL26 isolated controls/restorations passed; no compiler/tooling failure counted', flush=True)
