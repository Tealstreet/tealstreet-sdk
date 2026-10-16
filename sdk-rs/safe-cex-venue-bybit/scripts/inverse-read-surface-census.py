"""Independent file-qualified committed-object eight-row census."""
from pathlib import Path
import collections,json,subprocess,sys,tomllib
ROOT=Path(__file__).resolve().parents[3]
def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT,text=True).strip()
base=git('rev-parse',sys.argv[1]if len(sys.argv)>1 else '0445a753756f197e4363222eab9e787868e4f700');head=git('rev-parse','HEAD');prefix='sdk-rs/parity/manifest/exchanges/bybit';files=git('ls-tree','-r','--name-only',base,'--',prefix).splitlines();assert len(files)==26
assert git('ls-tree','-r','--name-only',head,'--',prefix).splitlines()==files, 'manifest file inventory changed'
before={};after={}
for file in files:
 for sha,table in [(base,before),(head,after)]:
  for row in tomllib.loads(git('show',sha+':'+file))['symbol']:
   key=(file,row['ts']);assert key not in table;table[key]=row
assert before.keys()==after.keys() and len(before)==520
expected={('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.formatBalance'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.fetchMarkets'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.get incomeContext'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.fetchIncomeHistory'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.fetchRawBalance'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.fetchOrdersHelper'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.fetchIncomeHistoryRange'), ('sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml', 'BybitInverseExchange.get cachedApiKeyInfo')}
moves=[]
for key,row in after.items():
 old=before[key]
 if old['status']!=row['status']:
  assert key in expected and (old['status'],row['status'])==('pending','ported'),(key,old['status'],row['status']);targets=row['rs']if isinstance(row['rs'],list)else[row['rs']];assert all(t.startswith('safe-cex-venue-bybit/src/inverse_read_surface.rs::')for t in targets);moves.append(dict(file=key[0],symbol=key[1],before=old['status'],after=row['status']))
assert {(r['file'],r['symbol'])for r in moves}==expected
def counts(table):return dict(collections.Counter(row['status']for row in table.values()))
r=dict(head=head,base=base,before=counts(before),after=counts(after),pending_to_ported=8,pending_to_divergent=0,regressions=0,manifest_floor=26,row_floor=520,last_manifest=files[-1],moves=moves);logs=ROOT/'.aimux/bybit-inverse-reads';logs.mkdir(exist_ok=True,parents=True);(logs/'census.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps({k:v for k,v in r.items()if k!='moves'}))
