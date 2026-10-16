import json,tomllib,subprocess,collections
from pathlib import Path
base='a58e1e79223f01bd7abd93053e26f3294e120d01'
expected={
 'exchanges/base.ws.ts.toml':{'BaseWebSocket#reconnectNow':'divergent','BaseWebSocket.retryFailedConnect':'ported','BaseWebSocket.recoverAfterOffline':'divergent'},
 'exchanges/base.ts.toml':{'BaseExchange.recoverAfterOffline':'ported'},
 'exchanges/types.ts.toml':{'MessageHandlersStats':'divergent','MessageHandlers':'divergent'},
 'exchanges/fillEventUnits.ts.toml':{'fillEventUnits':'divergent','FILL_EVENT_UNIT_VENUES':'divergent'},
 'httpOverWebsocket/configureHOW.ts.toml':{'initializeHOW':'divergent'},
 'httpOverWebsocket/axiosHttpOverWebsocketAdapter.ts.toml':{'getServerLatencyForExchange':'divergent'}}
rows=[];old=collections.Counter();now=collections.Counter()
for f,spec in expected.items():
 path='sdk-rs/parity/manifest/'+f;a=tomllib.loads(subprocess.check_output(['git','show',base+':'+path],text=True));b=tomllib.loads(Path(path).read_text());aa={(x['ts'],x['kind']):x for x in a['symbol']};bb={(x['ts'],x['kind']):x for x in b['symbol']};assert aa.keys()==bb.keys()
 changed=set()
 for key in aa:
  before,after=aa[key],bb[key];old[before['status']]+=1;now[after['status']]+=1
  if before['status']!=after['status']:
   assert before['status']=='pending';assert spec[key[0]]==after['status'];assert after['rs'];assert after['status']!='divergent' or after['note'].split(':')[0]in['MIXED','CHOSEN','FORCED','CORRECTED','FINDING'];changed.add(key[0]);rows.append({'file':f,'ts':key[0],'status':after['status'],'rs':after['rs']})
  else:assert before==after,'untargeted row changed: '+repr(key)
 assert changed==spec.keys()
assert len(rows)==10;assert now['pending']==0
print(json.dumps({'base':base,'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'before':dict(old),'after':dict(now),'moved':{'ported':2,'divergent':8},'floor':'all symbol rows in the six explicitly requested manifests, no repository-wide census','rows':rows},indent=2))
