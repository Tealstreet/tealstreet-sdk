"""Independent fixed pending-symbol census against an explicit integration parent."""
from pathlib import Path
import sys,tomllib,subprocess,json,collections
parent=subprocess.check_output(['git','rev-parse',sys.argv[1]],text=True).strip();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();root=Path('sdk-rs/parity/manifest');files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml'];assert len(files)==22
expected=set('WooV3AccountMode WooV3AccountPositionMode WooV3PositionSide WooV3PageMeta WooV3AccountInfo WooV3AssetBalance WooV3AssetBalanceReadModel WooV3FuturesPosition WooV3PositionReadModel isRecord requireRecord requireArray requireString requireBoolean requireInteger requireFiniteNumber requireDecimalString optionalDecimalString requireEnum decodeEnvelope decodeMeta decodePaginatedRows decimalToNumber normalizePerpetualSymbol mapPositionMode assertPositionSideMatchesMode decodeWooV3AccountInfo mapWooV3AccountBalance decodeWooV3AssetBalances mapWooV3AssetBalances decodeWooV3FuturesPositions mapWooV3FuturesPositions'.split());assert len(expected)==32
ported={'WooV3AccountMode','WooV3AccountPositionMode','WooV3PositionSide','mapPositionMode'};before=collections.Counter();after=collections.Counter();moved=[]
for p in files:
 old=tomllib.loads(subprocess.check_output(['git','show',parent+':'+str(p)],text=True))['symbol'];new=tomllib.loads(p.read_text())['symbol'];assert [r['ts'] for r in old]==[r['ts'] for r in new]
 for a,b in zip(old,new):
  before[a['status']]+=1;after[b['status']]+=1
  if a['status']!=b['status']:
   assert a['status']=='pending' and b['status']==('ported' if b['ts'] in ported else 'divergent') and p.name=='woo.v3.private.ts.toml'
   moved.append({'file':str(p),'ts':b['ts'],'before':a['status'],'after':b['status'],'rs':b['rs']})
assert sum(before.values())==534 and sum(after.values())==534 and {r['ts'] for r in moved}==expected
print(json.dumps({'parent':parent,'sha':sha,'manifest_files':22,'rows':534,'before':dict(before),'after':dict(after),'rows_moved':32,'ported':4,'divergent':28,'moved':moved},indent=2))
