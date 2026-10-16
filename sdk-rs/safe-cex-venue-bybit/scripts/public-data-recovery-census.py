#!/usr/bin/env python3
"""Committed-object tomllib census; independent, file-qualified ten-row expectation."""
from pathlib import Path
import collections,json,subprocess,sys,tomllib
root=Path(__file__).resolve().parents[3]
def git(*args):return subprocess.check_output(['git',*args],cwd=root,text=True).strip()
base=git('rev-parse',sys.argv[1] if len(sys.argv)>1 else 'b82816a58e')
head=git('rev-parse','HEAD');prefix='sdk-rs/parity/manifest/exchanges/bybit'
expected={}
for name,pairs in {
 'bybit.exchange.ts': [('BybitExchange.fetchMarkets','fetch_markets'),('BybitExchange.fetchFees','fetch_fees'),('BybitExchange#fetchTradingMarketsResponse','fetch_trading_markets_response')],
 'bybit.ws-public.ts': [('BybitPublicWebsocket.onOpen','on_open'),('BybitPublicWebsocket#clearOhlcvNoDataTimeout','clear_ohlcv_no_data_timeout'),('BybitPublicWebsocket#armOhlcvNoDataTimeout','arm_ohlcv_no_data_timeout'),('BybitPublicWebsocket#armExistingOhlcvNoDataTimeouts','arm_existing_ohlcv_no_data_timeouts'),('BybitPublicWebsocket.maybeRecoverOhlcvStream','maybe_recover_ohlcv_stream'),('BybitPublicWebsocket#maybeRecoverOhlcvStreamFromTopics','maybe_recover_ohlcv_stream_from_topics'),('BybitPublicWebsocket#onMarketsChanged','on_markets_changed')],
}.items():
 for symbol,target in pairs:expected[(prefix+'/'+name+'.toml',symbol)]=('ported','safe-cex-venue-bybit/src/public_data_recovery.rs::'+target)
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
result=dict(measured_sha=head,base_sha=base,fresh_parent=git('rev-parse','b82816a58e'),before=dict(collections.Counter(r['status'] for r in before.values())),after=dict(collections.Counter(r['status'] for r in after.values())),pending_to_ported=ported,pending_to_divergent=divergent,moves=moves,manifest_floor=26,last_manifest=files[-1],row_floor=520)
p=root/'sdk-rs/safe-cex-venue-bybit/evidence/public-data-recovery-census.json';p.parent.mkdir(exist_ok=True);p.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='moves'}))
