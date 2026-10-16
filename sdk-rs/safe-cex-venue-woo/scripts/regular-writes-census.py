"""Fixed 32-symbol expectation, independently loaded integration parent and tomllib census."""
from pathlib import Path
import sys,tomllib,subprocess,json,collections
parent=subprocess.check_output(['git','rev-parse',sys.argv[1]],text=True).strip();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();root=Path('sdk-rs/parity/manifest');files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml'];assert len(files)==22
expected=set('WooV3RegularTimeInForce WooV3RegularWriteTarget WooV3RegularPlaceInput WooV3RegularPlaceResult WooV3RegularAmendInput WooV3RegularAmendBuildResult WooV3RegularMutationResult WooV3RegularCancelPendingResult WooV3WriteResponseError WooV3WriteResponseError.constructor WOO_V3_REGULAR_WRITE_PARITY requireRecord requireBoolean requireInteger requireString requireEnum requireDecimal decimalParts compareDecimals requirePerpetualSymbol validateTarget mapPlaceType buildWooV3RegularPlaceRequest buildWooV3RegularAmendRequest buildWooV3RegularCancelRequest buildWooV3RegularCancelPendingRequest decodeSuccessData decodeWooV3RegularPlaceResponse decodeMutationResponse decodeWooV3RegularAmendResponse decodeWooV3RegularCancelResponse decodeWooV3RegularCancelPendingResponse'.split());assert len(expected)==32
ported={'WooV3RegularTimeInForce'};before=collections.Counter();after=collections.Counter();moved=[];notes=[];targets=[]
for p in files:
 old=tomllib.loads(subprocess.check_output(['git','show',parent+':'+str(p)],text=True))['symbol'];new=tomllib.loads(p.read_text())['symbol'];assert [r['ts'] for r in old]==[r['ts'] for r in new]
 for a,b in zip(old,new):
  before[a['status']]+=1;after[b['status']]+=1
  if a['status']!=b['status']:
   assert a['status']=='pending' and b['status']==('ported' if b['ts'] in ported else 'divergent') and p.name=='woo.v3.regular-writes.ts.toml'
   assert b['rs'] and (b['status']=='ported' or b['note'].startswith('CHOSEN:'))
   moved.append({'ts':b['ts'],'before':a['status'],'after':b['status'],'rs':b['rs']});notes.append(b['note']);targets+=b['rs'] if isinstance(b['rs'],list) else [b['rs']]
assert sum(before.values())==534 and sum(after.values())==534 and {r['ts'] for r in moved}==expected and len(set(notes))==32
print(json.dumps({'parent':parent,'sha':sha,'manifest_files':22,'rows':534,'before':dict(before),'after':dict(after),'rows_moved':32,'ported':1,'divergent':31,'distinct_notes':32,'distinct_targets':len(set(targets)),'moved':moved},indent=2))
