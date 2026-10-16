#!/usr/bin/env python3
"""Representative native-table truncation plus inherited-probe admission controls."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
cmd = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
       '-p', 'safe-cex-venue-bingx', '--test', 'constants', '--jobs', '1', '--locked', '--', '--nocapture']
source = crate / 'src/constants.rs'
oracle = crate / 'tests/constants_oracle.cjs'
pattern = r'pub const ORDER_SIDE: \[\(&str, OrderSide\); 2\] = \[(.*?)\];'
controls = []
original = source.read_text()
match = re.search(pattern, original, re.S)
assert match
for count in [0, 1]:
    rows = [row + ',' for row in re.findall(r'\("[^"]+", OrderSide::\w+\)', match[1])][:count]
    replacement = f'pub const ORDER_SIDE: [(&str, OrderSide); {count}] = [\n' + '\n'.join(rows) + '\n];'
    controls.append((f'ORDER_SIDE/{count}', source, original[:match.start()] + replacement + original[match.end():], 'ordered constant ORDER_SIDE'))
original = oracle.read_text()
for expr in ['[]', 'keys.slice(1)']:
    before = 'inherited[name] = keys.map(key => {'
    assert before in original
    controls.append((f'probes/{expr}', oracle, original.replace(before, f'inherited[name] = {expr}.map(key => {{'), 'BINGX-CARDINALITY inherited probes'))
for label, path, changed, assertion in controls:
    original = path.read_bytes()
    try:
        path.write_text(changed)
        result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode == 101 and assertion in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'{label}: compiled; exit101 at {assertion}', flush=True)
    finally:
        path.write_bytes(original)
    result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '2 passed' in result.stdout, result.stdout
    print(f'{label}: restored 2 passed', flush=True)
