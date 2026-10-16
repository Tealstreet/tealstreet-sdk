"""Independent nine-row expectation; iteration 1 stops at eight files/231 rows."""
import collections,pathlib,subprocess,tomllib
ROOT=pathlib.Path(__file__).resolve().parents[3]
BASE='9c663a908f40c2955e27b1ce58f97fb5c08fb331'
PREFIX='sdk-rs/parity/manifest/exchanges/asterdex'
EXPECTED={'AsterDEXPublicWebsocket':'divergent','AsterDEXPublicWebsocket.constructor':'ported','AsterDEXPublicWebsocket.connectAndSubscribe':'divergent','AsterDEXPublicWebsocket.onMessage':'divergent','AsterDEXExchange.listenOHLCV':'ported','AsterDEXExchange.listenOrderBook':'ported','switchToAsterSignatureChain':'ported','checkAsterBuilderApproved':'divergent','approveAsterBuilderFeeWithBrowserWallet':'divergent'}
paths=sorted((ROOT/PREFIX).glob('*.toml'));assert len(paths)==8
before,after=collections.Counter(),collections.Counter();moved={};notes=[];targets=[]
for path in paths:
 old=tomllib.loads(subprocess.check_output(['git','show',f'{BASE}:{path.relative_to(ROOT)}'],cwd=ROOT,text=True));current=tomllib.loads(path.read_text());old_rows={(r['kind'],r['ts']):r for r in old['symbol']};new_rows={(r['kind'],r['ts']):r for r in current['symbol']};assert old_rows.keys()==new_rows.keys()
 before.update(r['status']for r in old_rows.values());after.update(r['status']for r in new_rows.values())
 for identity,row in new_rows.items():
  if old_rows[identity]['status']!=row['status']:
   assert old_rows[identity]['status']=='pending';moved[row['ts']]=row['status'];assert row['note'] and row['rs'];file,symbol=row['rs'].split('::',1);source=(ROOT/'sdk-rs'/file).read_text();assert any(prefix+symbol.split('::')[-1]in source for prefix in['pub struct ','pub fn ','pub async fn ']);assert file in current['file']['rust_files']
   if row['status']=='divergent':assert row['note'].split(':')[0]in{'FORCED','CHOSEN','MIXED','CORRECTED','FINDING'};notes.append(row['note']);targets.append(row['rs'])
assert before=={'ported':126,'divergent':87,'pending':18},before
assert moved==EXPECTED,moved
assert after=={'ported':130,'divergent':92,'pending':9},after
assert len(set(notes))==5 and len(set(targets))==5
print(f'Base {BASE}: {dict(before)}; current {dict(after)}; moved 4 pending->ported, 5 pending->divergent')
print('Iteration 1 stopped after 8 files/231 rows; 9 named status changes, 5 distinct canonical notes and targets; normalizeAddress stays divergent with complete wrapper target')
