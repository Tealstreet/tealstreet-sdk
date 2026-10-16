"""File-qualified committed-object census for the nine Hyro socket rows."""
from pathlib import Path
import collections, json, subprocess, sys, tomllib
ROOT=Path(__file__).resolve().parents[3]
def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT,text=True).strip()
base=git('rev-parse',sys.argv[1] if len(sys.argv)>1 else 'f6c865ddd8');head=git('rev-parse','HEAD');prefix='sdk-rs/parity/manifest/exchanges/bybit'
files=git('ls-tree','-r','--name-only',base,'--',prefix).splitlines();assert len(files)==26
before={};after={}
for file in files:
 for sha,table in [(base,before),(head,after)]:
  for row in tomllib.loads(git('show',sha+':'+file))['symbol']:
   key=(file,row['ts']);assert key not in table;table[key]=row
assert before.keys()==after.keys() and len(before)==520
selected=prefix+'/hyrotrader.ws-private.ts.toml'
expected={(selected,name) for name in ['HyroPrivateWebsocket','HyroPrivateWebsocket.get sessionVersion','HyroPrivateWebsocket.get isRecoveryComplete','HyroPrivateWebsocket.constructor','HyroPrivateWebsocket#resetSession','HyroPrivateWebsocket.get resyncsPrivateStateOnReopen','HyroPrivateWebsocket.sendReconnectSequence','HyroPrivateWebsocket#request','HyroPrivateWebsocket.dispose']}
moves=[]
for key,row in after.items():
 old=before[key]
 if row['status']!=old['status']:
  assert key in expected and (old['status'],row['status'])==('pending','ported'),(key,old['status'],row['status'])
  targets=row['rs'] if isinstance(row['rs'],list)else[row['rs']];assert all(t.startswith('safe-cex-venue-bybit/src/hyro_private_session.rs::') for t in targets)
  moves.append(dict(file=key[0],symbol=key[1],before=old['status'],after=row['status']))
assert {(r['file'],r['symbol'])for r in moves}==expected
ours={'bybitinverse.exchange.ts.toml','bybitinverse.ws-private.ts.toml','bybitinverse.ws-public.ts.toml','bybitspot.exchange.ts.toml','bybitspot.ws-private.ts.toml','bybitspot.ws-public.ts.toml','hyrotrader.exchange.ts.toml','hyrotrader.order-recovery.ts.toml','hyrotrader.ws-private.ts.toml','bybitdemo.exchange.ts.toml'}
def triple(table,own=False):return dict(collections.Counter(r['status'] for (file,_),r in table.items() if not own or Path(file).name in ours))
report=dict(head=head,base=base,fresh_parent=git('rev-parse','f6c865ddd8'),before=triple(before),after=triple(after),partition_before=triple(before,True),partition_after=triple(after,True),pending_to_ported=9,pending_to_divergent=0,regressions=0,moves=moves,manifest_floor=26,row_floor=520,last_manifest=files[-1])
logs=ROOT/'.aimux/bybit-variants';logs.mkdir(parents=True,exist_ok=True);(logs/'hyro-census.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='moves'}))
