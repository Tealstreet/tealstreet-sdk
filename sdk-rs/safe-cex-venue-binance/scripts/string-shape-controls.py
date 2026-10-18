#!/usr/bin/env python3
"""Prove each string frontdoor against an otherwise valid unit tagged object."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-binance'
cases = [
    ('generated/binance.rs', 'BinanceIncomeType', 'income_type_rejects_each_valid_tagged_object'),
    ('protocol_types.rs', 'ApiType', 'api_type_rejects_each_valid_tagged_object'),
    ('protocol_types.rs', 'BinanceOrderBookApplyResult', 'orderbook_apply_rejects_each_valid_tagged_object'),
]
originals = {crate / 'src' / file: (crate / 'src' / file).read_bytes() for file, _, _ in cases}


def run(test):
    result = subprocess.run(
        ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-binance', '--jobs', '1',
         '--test', 'accepted_string_shapes', test, '--', '--exact'],
        cwd=workspace, capture_output=True, text=True, timeout=600,
    )
    return result, result.stdout + result.stderr


try:
    for file, owner, test in cases:
        path = crate / 'src' / file
        original = originals[path]
        marker = ("impl<'de> serde::Deserialize<'de> for " + owner + ' {').encode()
        assert original.count(marker) == 1, owner
        implementation = original.index(marker)
        start = original.index(b'        let text =', implementation)
        end = original.index(b'\n    }', start)
        body = original[start:end]
        assert b'<String as serde::Deserialize' in body and (owner + 'Wire::deserialize').encode() in body
        assert b'StringDeserializer' in body
        replacement = ('        ' + owner + 'Wire::deserialize(deserializer)').encode()
        path.write_bytes(original[:start] + replacement + original[end:])
        result, output = run(test)
        assert (result.returncode == 101
                and f'test {test} ... FAILED' in output
                and 'valid unit tag accepted as object:' in output
                and 'route=0' in output and 'error[E' not in output), output[-6000:]
        print(f'CONTROL PASS: {owner} compiled; removing only String admission accepts a valid tagged object', flush=True)
        path.write_bytes(original)
        restored, output = run(test)
        assert restored.returncode == 0 and '1 passed' in output, output[-6000:]
        print(f'RESTORED PASS: {owner}', flush=True)
finally:
    for path, source in originals.items():
        path.write_bytes(source)
