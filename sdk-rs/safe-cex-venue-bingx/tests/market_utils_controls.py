#!/usr/bin/env python3
"""Sequential controls; each target executes one complete live Node corpus."""
import os
import re
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/market_utils.rs'
oracle = crate / 'tests/market_utils_oracle.cjs'
target = crate / 'tests/market_utils.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'market_utils', '--jobs', '1', '--locked']
controls = [
    ('classification', [(source, 'folded.ends_with("usd")', '!folded.ends_with("usd")')], 'native classification'),
    ('precedence', [(source, 'match js_table_lookup(markets_by_symbol, symbol)', 'match js_table_lookup(markets_by_id, symbol)'), (source, 'TableHit::Missing => js_table_lookup(markets_by_id, symbol)', 'TableHit::Missing => js_table_lookup(markets_by_symbol, symbol)')], 'native resolution'),
    ('empty-id', [(source, 'hit => hit,', 'TableHit::Mapped(market) if market.id.is_empty() => js_table_lookup(markets_by_id, symbol),\n        hit => hit,')], 'native resolution'),
    ('inherited', [(source, 'TableHit::Missing =>', 'TableHit::Missing | TableHit::Inherited(_) =>')], 'native resolution'),
]
for group, count, label in [('classify', 75, 'classification'), ('resolve', 138, 'resolution')]:
    for operation in ['splice(0)', 'splice(1)', 'push(' + group + '[0])']:
        for bypass in [False, True]:
            edits = [(oracle, 'process.stdout.write', f'{group}.{operation};\nprocess.stdout.write')]
            if bypass:
                # Remove only admission guard; executed-comparison guard must still reject it.
                guard = re.search(r'assert_eq!\(\s*' + group + r'\.len\(\),\s*' + str(count) + r',\s*"BINGX-MARKET ' + label + r' source total"\s*\);', target.read_text()).group()
                edits.append((target, guard, ''))
            controls.append((f'{group}-{operation}-bypass-{bypass}', edits,
                             f'{label} comparisons' if bypass else f'{label} source total'))
for name, edits, assertion in controls:
    originals = {path: path.read_bytes() for path, _, _ in edits}
    try:
        for path, before, after in edits:
            text = path.read_text()
            assert text.count(before) == 1, (name, before)
            path.write_text(text.replace(before, after))
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode == 101 and f'BINGX-MARKET {assertion}' in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'{name}: exit 101 BINGX-MARKET {assertion}', flush=True)
    finally:
        for path, original in originals.items():
            path.write_bytes(original)
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
    print('restored: 1 passed', flush=True)
