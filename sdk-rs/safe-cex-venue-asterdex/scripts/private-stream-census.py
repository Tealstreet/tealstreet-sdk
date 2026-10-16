"""Independent eight-row expectation, iteration 1 stopped at eight ASTERDEX files."""
import collections,pathlib,subprocess,tomllib
ROOT=pathlib.Path(__file__).resolve().parents[3]
BASE='d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e'
PREFIX='sdk-rs/parity/manifest/exchanges/asterdex'
EXPECTED={
'AsterDEXPrivateWebsocket':'divergent',
'AsterDEXPrivateWebsocket.constructor':'ported',
'AsterDEXPrivateWebsocket.connectAndSubscribe':'divergent',
'AsterDEXPrivateWebsocket.onMessage':'divergent',
'AsterDEXPrivateWebsocket#handleOrderTradeUpdate':'divergent',
'AsterDEXPrivateWebsocket#applyBalanceUpdates':'divergent',
'AsterDEXPrivateWebsocket#handleAccountPositionUpdate':'divergent',
'AsterDEXPrivateWebsocket.dispose':'divergent',
}
paths=sorted((ROOT/PREFIX).glob('*.toml'));assert len(paths)==8
before,after=collections.Counter(),collections.Counter();moved={};notes=[];targets=[]
for path in paths:
 old=tomllib.loads(subprocess.check_output(['git','show',f'{BASE}:{path.relative_to(ROOT)}'],cwd=ROOT,text=True));current=tomllib.loads(path.read_text());old_rows={(r['kind'],r['ts']):r for r in old['symbol']};new_rows={(r['kind'],r['ts']):r for r in current['symbol']};assert old_rows.keys()==new_rows.keys()
 before.update(r['status']for r in old_rows.values());after.update(r['status']for r in new_rows.values())
 for identity,row in new_rows.items():
  if old_rows[identity]['status']!=row['status']:
   assert old_rows[identity]['status']=='pending';moved[row['ts']]=row['status'];assert row['note'] and row['rs'];file,symbol=row['rs'].split('::',1);source=(ROOT/'sdk-rs'/file).read_text();assert any(prefix+symbol.split('::')[-1]in source for prefix in['pub struct ','pub fn ']);assert file in current['file']['rust_files']
   if row['status']=='divergent':assert row['note'].split(':')[0]in{'FORCED','CHOSEN','MIXED','CORRECTED','FINDING'};notes.append(row['note']);targets.append(row['rs'])
assert before=={'ported':125,'divergent':80,'pending':26},before
assert moved==EXPECTED,moved
assert after=={'ported':126,'divergent':87,'pending':18},after
assert len(set(notes))==7 and len(set(targets))==7
print(f'Base {BASE}: {dict(before)}; current {dict(after)}; moved 1 pending->ported, 7 pending->divergent')
print('Iteration 1 stopped after 8 files/231 rows; 8 named changes, 7 distinct canonical notes and targets')
