#!/usr/bin/env python3
"""Prove each record guard against its complete independently authored array."""
from pathlib import Path
import re
import subprocess

workspace = Path(__file__).resolve().parents[2]
generated = workspace / 'safe-cex-venue-binance/src/generated'
cases = [
    ('binance.rs', 'GetPositionRiskResponseRow', 'futures_risk_rejects_complete_positional_array'),
    ('binancepm.rs', 'GetPositionRiskResponseRow', 'pm_risk_rejects_complete_positional_array'),
    ('binance.rs', 'BinanceIncomeRecord', 'income_record_rejects_complete_positional_array'),
    ('binance.rs', 'BinanceIncomeHistoryOptions', 'income_options_rejects_complete_positional_array'),
]
originals = {generated / file: (generated / file).read_bytes() for file, _, _ in cases}


def run(test):
    result = subprocess.run(
        ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-binance', '--jobs', '1',
         '--test', 'accepted_record_shapes', test, '--', '--exact'],
        cwd=workspace, capture_output=True, text=True, timeout=600,
    )
    return result, result.stdout + result.stderr


try:
    for file, owner, test in cases:
        path = generated / file
        original = originals[path]
        pattern = (re.escape(owner) + r'Wire::deserialize\(\s*'
                   r'safe_cex_types::strict_decode::object\(\s*decoder\s*,?\s*\)\s*,?\s*\)').encode()
        mutant, count = re.subn(pattern, (owner + 'Wire::deserialize(decoder)').encode(), original)
        assert count == 1, f'{file}::{owner}: expected exactly one object guard'
        path.write_bytes(mutant)
        result, output = run(test)
        assert (result.returncode == 101
                and f'test {test} ... FAILED' in output
                and 'complete valid positional array or non-object accepted: route=0 input=[' in output
                and 'error[E' not in output), output[-6000:]
        print(f'CONTROL PASS: {file}::{owner} compiled; complete valid array fails its named assertion', flush=True)
        path.write_bytes(original)
        restored, output = run(test)
        assert restored.returncode == 0 and '1 passed' in output, output[-6000:]
        print(f'RESTORED PASS: {file}::{owner}', flush=True)
finally:
    for path, source in originals.items():
        path.write_bytes(source)
