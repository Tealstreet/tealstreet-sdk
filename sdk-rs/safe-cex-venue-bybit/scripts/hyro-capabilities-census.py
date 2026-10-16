"""Independent file-qualified committed-object ten-row census."""
from pathlib import Path
import collections,json,subprocess,sys,tomllib
ROOT=Path(__file__).resolve().parents[3]
def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT,text=True).strip()
base=git('rev-parse',sys.argv[1]if len(sys.argv)>1 else 'a07e971bb0');head=git('rev-parse','HEAD');prefix='sdk-rs/parity/manifest/exchanges/bybit';files=git('ls-tree','-r','--name-only',base,'--',prefix).splitlines();assert len(files)==26
before={};after={}
for file in files:
 for sha,table in [(base,before),(head,after)]:
  for row in tomllib.loads(git('show',sha+':'+file))['symbol']:
   key=(file,row['ts']);assert key not in table;table[key]=row
assert before.keys()==after.keys() and len(before)==520
expected={(prefix+'/hyrotrader.exchange.ts.toml','HyrotraderExchange'+name)for name in ['#fail','.fetchAndCacheApiKeyInfo','.fetchApiKeyInfo','.getAccount','.fetchFees','.fetchPositionMode','.changePositionMode','.changeMarginType','#assertPrivateReady','.validateOrderCapabilities']}
moves=[]
for key,row in after.items():
 old=before[key]
 if old['status']!=row['status']:
  assert key in expected and (old['status'],row['status'])==('pending','ported'),(key,old['status'],row['status']);targets=row['rs']if isinstance(row['rs'],list)else[row['rs']];assert all(t.startswith('safe-cex-venue-bybit/src/hyro_capabilities.rs::')for t in targets);moves.append(dict(file=key[0],symbol=key[1],before=old['status'],after=row['status']))
assert {(r['file'],r['symbol'])for r in moves}==expected
ours={'bybitinverse.exchange.ts.toml','bybitinverse.ws-private.ts.toml','bybitinverse.ws-public.ts.toml','bybitspot.exchange.ts.toml','bybitspot.ws-private.ts.toml','bybitspot.ws-public.ts.toml','hyrotrader.exchange.ts.toml','hyrotrader.order-recovery.ts.toml','hyrotrader.ws-private.ts.toml','bybitdemo.exchange.ts.toml'}
def counts(table,own=False):return dict(collections.Counter(row['status']for(file,_),row in table.items()if not own or Path(file).name in ours))
r=dict(head=head,base=base,fresh_parent=git('rev-parse','a07e971bb0'),before=counts(before),after=counts(after),partition_before=counts(before,True),partition_after=counts(after,True),pending_to_ported=10,pending_to_divergent=0,regressions=0,manifest_floor=26,row_floor=520,last_manifest=files[-1],moves=moves);logs=ROOT/'.aimux/bybit-capabilities';logs.mkdir(exist_ok=True,parents=True);(logs/'census.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps({k:v for k,v in r.items()if k!='moves'}))
