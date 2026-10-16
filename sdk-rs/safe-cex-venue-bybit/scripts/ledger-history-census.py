#!/usr/bin/env python3
"""Committed-object tomllib census; independent, file-qualified ten-row expectation."""
from pathlib import Path
import collections,json,subprocess,sys,tomllib
root=Path(__file__).resolve().parents[3]
def git(*args):return subprocess.check_output(['git',*args],cwd=root,text=True).strip()
base=git('rev-parse',sys.argv[1] if len(sys.argv)>1 else '7d61bf63d8')
head=git('rev-parse','HEAD');prefix='sdk-rs/parity/manifest/exchanges/bybit'
expected={}
for file, symbol, target in [
 ('bybit.exchange.ts','BybitExchange.fetchClosedPositionRows','fetch_closed_position_rows'),
 ('bybit.exchange.ts','BybitExchange.fetchHistoricalPositions','fetch_historical_positions'),
 ('bybit.exchange.ts','BybitExchange.fetchClosedPnl','fetch_closed_pnl'),
 ('bybit.exchange.ts','BybitExchange.fetchHistoricalOrders','fetch_historical_orders'),
 ('bybit.exchange.ts','BybitExchange.fetchIncomeHistory','fetch_income_history'),
 ('bybit.exchange.ts','BybitExchange.fetchIncomeHistoryRange','fetch_income_history_range'),
 ('bybit.exchange.ts','BybitExchange.get incomeContext','income_context'),
 ('bybit.income.ts','BybitIncomeContext','BybitIncomeContext'),
 ('bybit.income.ts','fetchBybitIncomeHistory','fetch_bybit_income_history'),
 ('bybit.income.ts','fetchBybitIncomeHistoryRange','fetch_bybit_income_history_range'),
]:expected[(prefix+'/'+file+'.toml',symbol)]=('ported','safe-cex-venue-bybit/src/ledger_history.rs::'+target)
assert len(expected)==10
files=git('ls-tree','-r','--name-only',base,'--',prefix).splitlines();assert len(files)==26
before={};after={}
for file in files:
 for revision,table in [(base,before),(head,after)]:
  for row in tomllib.loads(git('show',revision+':'+file))['symbol']:
   key=(file,row['ts']);assert key not in table;table[key]=row
assert len(before)==520 and before.keys()==after.keys()
moves=[]
for key,new in after.items():
 old=before[key]
 if new['status']!=old['status']:
  assert old['status']=='pending' and key in expected,(key,old['status'],new['status'])
  assert (new['status'],new['rs'])==expected[key]
  if new['status']=='divergent':assert new['note'].startswith('CHOSEN:')
  moves.append(dict(file=key[0],symbol=key[1],before=old['status'],after=new['status'],target=new['rs']))
assert {(m['file'],m['symbol']) for m in moves}==expected.keys()
ported=sum(m['after']=='ported' for m in moves);divergent=sum(m['after']=='divergent' for m in moves);assert (ported,divergent)==(10,0)
result=dict(measured_sha=head,base_sha=base,fresh_parent=git('rev-parse','7d61bf63d8'),before=dict(collections.Counter(r['status'] for r in before.values())),after=dict(collections.Counter(r['status'] for r in after.values())),pending_to_ported=ported,pending_to_divergent=divergent,moves=moves,manifest_floor=26,last_manifest=files[-1],row_floor=520)
p=root/'sdk-rs/safe-cex-venue-bybit/evidence/ledger-history-census.json';p.parent.mkdir(exist_ok=True);p.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='moves'}))
