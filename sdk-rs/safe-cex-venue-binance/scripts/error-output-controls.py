#!/usr/bin/env python3
"""Prove each observed decoder output against its actual production function."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-binance'
runner = crate / 'scripts/error-output-contracts.mjs'
cases = [
    ('response_message.rs', 'get_binance_response_message',
     'safe_cex_venue_binance::response_message::get_binance_response_message',
     b'\n    let _ = payload;\n    Ok(String::new())\n'),
    ('response_message.rs', 'get_binance_pm_response_message',
     'safe_cex_venue_binance::response_message::get_binance_pm_response_message',
     b'\n    let _ = payload;\n    Ok(String::new())\n'),
    ('response_message.rs', 'response_message',
     'safe_cex_venue_binance::response_message::response_message',
     b'\n    let _ = payload.map(from_json);\n    Ok(String::new())\n'),

]
originals = {crate / 'src' / file: (crate / 'src' / file).read_bytes() for file, _, _, _ in cases}


def run(owner=None):
    args = ['node', str(runner)]
    if owner:
        args.append('--expect-output-failure=' + owner)
    result = subprocess.run(args, cwd=workspace, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


try:
    for file, function, owner, replacement in cases:
        path = crate / 'src' / file
        original = originals[path]
        marker = ('fn ' + function + '(').encode()
        assert original.count(marker) == 1, owner
        start = original.index(b'{', original.index(marker))
        end = start + 1
        depth = 1
        # Reviewed bodies contain only balanced braces in format-string literals.
        while depth:
            depth += (original[end] == ord('{')) - (original[end] == ord('}'))
            end += 1
        path.write_bytes(original[:start + 1] + replacement + original[end - 1:])
        changed, output = run(owner)
        assert (changed.returncode == 0 and f'NATIVE_OUTPUT_FAILURE: {owner}' in output
                and 'compile failure is not a control' not in output), output[-8000:]
        print(f'CONTROL PASS: {owner} compiled; its direct production output witness fails', flush=True)
        path.write_bytes(original)
        restored, output = run()
        assert (restored.returncode == 0 and 'Three production decoder-output probes passed' in output
                and '0 failed owners' in output), output[-8000:]
        print(f'RESTORED PASS: {owner}; all three actual production outputs pass', flush=True)
finally:
    for path, source in originals.items():
        path.write_bytes(source)
