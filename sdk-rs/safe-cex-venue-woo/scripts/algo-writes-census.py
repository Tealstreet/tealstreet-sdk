"""Fixed 43-symbol expectation, independently loaded integration parent and tomllib census."""
from pathlib import Path
import sys,tomllib,subprocess,json,collections
parent=subprocess.check_output(['git','rev-parse',sys.argv[1]],text=True).strip();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();root=Path('sdk-rs/parity/manifest');files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml'];assert len(files)==22
expected=set('WooV3AlgoWriteTarget WooV3AlgoTriggerPriceType WooV3StopMarketPlaceInput WooV3TrailingPlaceInput WooV3PositionalTriggerInput WooV3PositionalPlaceInput WooV3AlgoPlaceInput WooV3AlgoPlaceBuildResult WooV3AlgoPlaceResponseRow WooV3AlgoPlaceDecodeResult WooV3AlgoAmendInput WooV3AlgoAmendBuildResult WooV3AlgoMutationResult WooV3AlgoCancelPendingBuildResult WooV3AlgoCancelPendingResult WooV3AlgoWriteResponseError WooV3AlgoWriteResponseError.constructor WOO_V3_ALGO_WRITE_PARITY requireRecord requireArray requireBoolean requireInteger requireString requireEnum requireDecimal decimalParts compareDecimals requirePerpetualSymbol validateTarget validateCommon hasUnsupportedTrigger buildWooV3AlgoPlaceRequest decodeSuccessData decodePlaceRow decodeWooV3AlgoPlaceResponse targetMatchesOrder buildWooV3AlgoAmendRequest buildWooV3AlgoCancelRequest buildWooV3AlgoCancelPendingRequest decodeMutation decodeWooV3AlgoAmendResponse decodeWooV3AlgoCancelResponse decodeWooV3AlgoCancelPendingResponse'.split());assert len(expected)==43
ported={'WooV3AlgoTriggerPriceType','hasUnsupportedTrigger'};before=collections.Counter();after=collections.Counter();moved=[];notes=[];targets=[]
for p in files:
 old=tomllib.loads(subprocess.check_output(['git','show',parent+':'+str(p)],text=True))['symbol'];new=tomllib.loads(p.read_text())['symbol'];assert [r['ts'] for r in old]==[r['ts'] for r in new]
 for a,b in zip(old,new):
  before[a['status']]+=1;after[b['status']]+=1
  if a['status']!=b['status']:
   assert a['status']=='pending' and b['status']==('ported' if b['ts'] in ported else 'divergent') and p.name=='woo.v3.algo-writes.ts.toml'
   assert b['rs'] and (b['status']=='ported' or b['note'].startswith('CHOSEN:'))
   moved.append({'ts':b['ts'],'before':a['status'],'after':b['status'],'rs':b['rs']});notes.append(b['note']);targets+=b['rs'] if isinstance(b['rs'],list) else [b['rs']]
assert sum(before.values())==534 and sum(after.values())==534 and {r['ts'] for r in moved}==expected and len(set(notes))==43
print(json.dumps({'parent':parent,'sha':sha,'manifest_files':22,'rows':534,'before':dict(before),'after':dict(after),'rows_moved':43,'ported':2,'divergent':41,'distinct_notes':43,'distinct_targets':len(set(targets)),'moved':moved},indent=2))
