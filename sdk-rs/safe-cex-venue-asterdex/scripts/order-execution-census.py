"""Independent eight-row expectation; iteration 1, eight files/231 rows."""
import collections
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = 'a58e1e7922'
PREFIX = 'sdk-rs/parity/manifest/exchanges/asterdex'
EXPECTED={
'AsterDEXExchange.validateAccount':'divergent',
'AsterDEXExchange.placeOrder':'divergent',
'AsterDEXExchange.placeOrders':'divergent',
'AsterDEXExchange.updateOrder':'divergent',
'AsterDEXExchange.cancelOrders':'divergent',
'AsterDEXExchange#updateStoreOpenInterest':'ported',
'AsterDEXExchange#pollOpenInterest':'divergent',
'AsterDEXExchange#startOpenInterestPolling':'divergent',
}
paths = sorted((ROOT / PREFIX).glob('*.toml'))
assert len(paths) == 8
before, after = collections.Counter(), collections.Counter()
moved, notes, targets = {}, [], []
for path in paths:
    old = tomllib.loads(subprocess.check_output(['git', 'show', f'{BASE}:{path.relative_to(ROOT)}'], cwd=ROOT, text=True))
    current = tomllib.loads(path.read_text())
    old_rows = {(r['kind'], r['ts']): r for r in old['symbol']}
    new_rows = {(r['kind'], r['ts']): r for r in current['symbol']}
    assert old_rows.keys() == new_rows.keys()
    before.update(r['status'] for r in old_rows.values())
    after.update(r['status'] for r in new_rows.values())
    for identity, row in new_rows.items():
        if old_rows[identity]['status'] != row['status']:
            assert old_rows[identity]['status'] == 'pending'
            moved[row['ts']] = row['status']
            assert row['rs'] and row['note']
            file, symbol = row['rs'].split('::', 1)
            source = (ROOT / 'sdk-rs' / file).read_text()
            assert any(prefix+symbol.split('::')[-1] in source for prefix in ['pub async fn ','pub fn '])
            assert file in current['file']['rust_files']
            if row['status'] == 'divergent':
                assert row['note'].split(':')[0] in {'FORCED','CHOSEN','MIXED','CORRECTED','FINDING'}
                notes.append(row['note']); targets.append(row['rs'])
assert before == {'ported': 124, 'divergent': 73, 'pending': 34}, before
assert moved == EXPECTED, moved
assert after == {'ported': 125, 'divergent': 80, 'pending': 26}, after
assert len(set(notes)) == 7 and len(set(targets)) == 7
print(f'Base {BASE}: {dict(before)}')
print(f'Current: {dict(after)}; 1 pending->ported, 7 pending->divergent')
print('Iteration 1 stopped after 8 files/231 rows and 8 independent named changes; seven distinct bucketed notes/targets')
