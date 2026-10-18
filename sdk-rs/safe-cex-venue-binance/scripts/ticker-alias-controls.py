#!/usr/bin/env python3
"""Require compiled native alias dataflow failures and exact source restoration."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / 'src/ticker_capture.rs'
original = source.read_bytes()
logs = workspace / 'target/ticker-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'production_native_ticker_capture_aliases_retain_cells_and_replace_generations'
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes', name, '--', '--exact']
receipt = {'sourceSha256': hashlib.sha256(original).hexdigest(), 'controls': []}


def execute(label):
    path = logs / (label + '.log')
    with path.open('w') as output:
        result = subprocess.run(command, cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


status, log = execute('baseline-green')
assert status == 0 and '1 passed; 0 failed' in log
for alias, before, after in [
    ('Dictionary', '        self.dictionary.borrow().clone()',
     '        Rc::new(RefCell::new(self.dictionary.borrow().borrow().clone()))'),
    ('TickerReference', '                *row.borrow_mut() = ticker.clone();',
     '                let _ = (row, ticker);'),
]:
    assert source.read_bytes() == original
    text = original.decode()
    assert text.count(before) == 1, alias
    try:
        source.write_text(text.replace(before, after))
        status, log = execute(alias + '-red')
        assert status != 0, alias + ': corrupted alias dataflow passed'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), alias + ': compiler errors do not count'
        assert f'test {name} ... FAILED' in log, alias + ': named runtime assertion must fail'
        assert '0 passed; 1 failed' in log
        receipt['controls'].append({'owner': 'safe_cex_venue_binance::ticker_capture::' + alias,
                                    'test': name, 'namedRuntimeFailure': True})
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(alias + '-exact-restoration-green')
    assert status == 0 and '1 passed; 0 failed' in log, alias
    print(alias + ': compiled named native RED; exact restoration GREEN', flush=True)
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
assert source.read_bytes() == original
print('Both alias state-flow controls fail for real and restore exactly.', flush=True)
