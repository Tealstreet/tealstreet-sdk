#!/usr/bin/env python3
"""Production collection and source-corpus truncation controls, no per-case processes."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/types.rs'
oracle = crate / 'tests/types_oracle.cjs'
test = crate / 'tests/types.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
cmd = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
       '-p', 'safe-cex-venue-bingx', '--test', 'types', '--jobs', '1', '--locked', '--', '--nocapture']

def prove(label, path, changed, assertion):
    original = path.read_bytes()
    try:
        assert changed != original.decode()
        path.write_text(changed)
        result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode == 101 and assertion in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'{label}: compiled; exit101 at {assertion}', flush=True)
    finally:
        path.write_bytes(original)
    result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
    print(f'{label}: restored 1 passed', flush=True)

for name in ['FetchBalanceResponse', 'FetchPositionsResponse']:
    for label, action in [('empty', 'clear()'), ('subset', 'truncate(1)')]:
        original = source.read_text()
        start = original.index("impl<'de> Deserialize<'de> for " + name + ' {')
        end = original.index('\n}', start)
        part = original[start:end]
        part = part.replace('.map_err(serde::de::Error::custom)',
                            '.map(|mut value| { value.data.' + action + '; value })\n            .map_err(serde::de::Error::custom)')
        assertion = 'BINGX-CARDINALITY native array length'
        prove(name + '/' + label, source, original[:start] + part + original[end:], assertion)
before = 'names: Object.keys(declarations), cases, heritage: true'
for label, replacement in [('empty', 'cases: []'), ('subset', "cases: cases.filter(c => c.name !== 'FETCH_BALANCE_RESPONSE')")]:
    original = oracle.read_text()
    assert before in original
    prove('source/' + label, oracle, original.replace(before, 'names: Object.keys(declarations), ' + replacement + ', heritage: true'), 'BINGX-CARDINALITY total cases')

original = oracle.read_text()
# Preserve the total while deleting every case for one declaration.
replacement = "cases: cases.filter(c => c.name !== 'FETCH_BALANCE_RESPONSE').concat(Array(153).fill(cases[0]))"
prove('source/subset-same-total', oracle, original.replace(before, 'names: Object.keys(declarations), ' + replacement + ', heritage: true'), 'BINGX-CARDINALITY per-type cases')

original = oracle.read_text()
marker = 'process.stdout.write(JSON.stringify('
assert original.count(marker) == 1
for label, adjustment, assertion in [
    ('source/output-empty', "for (const c of cases) if (c.native && Array.isArray(c.native.data)) c.native.data = [];", 'BINGX-CARDINALITY source total rows'),
    ('source/output-redistribute', "const rows = cases.filter(c => c.native && Array.isArray(c.native.data)); rows[0].native.data.pop(); rows[1].native.data.push(rows[1].native.data[0]);", 'BINGX-CARDINALITY source per-case rows'),
]:
    prove(label, oracle, original.replace(marker, adjustment + '\n' + marker), assertion)
