"""Independent fixed ten-row expectation; compare tomllib census with an explicit integration parent."""
import subprocess,tomllib,json,collections,sys
from pathlib import Path
parent=subprocess.check_output(['git','rev-parse',sys.argv[1]],text=True).strip();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();root=Path('sdk-rs/parity/manifest');files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml'];assert len(files)==22
expected=set('WooV3CompletePositionModeEvidence WooV3SetPositionModeInput WooV3SetPositionModeBuildResult validateSnapshotEvidence isExactDecimalZero positionProvesNoExposure buildWooV3SetPositionModeRequest decodeWooV3SetPositionModeResponse WooV3CancelAllAfterRequest WooV3CancelAllAfterResponse'.split());assert len(expected)==10
before=collections.Counter();after=collections.Counter();moved=[];notes=[];targets=[]
for p in files:
 old=tomllib.loads(subprocess.check_output(['git','show',parent+':'+str(p)],text=True))['symbol'];new=tomllib.loads(p.read_text())['symbol'];assert [r['ts']for r in old]==[r['ts']for r in new]
 for a,b in zip(old,new):
  before[a['status']]+=1;after[b['status']]+=1
  if a['status']!=b['status']:
   assert a['status']=='pending'and b['status']=='divergent'and b['ts']in expected and b['note'].startswith('CHOSEN:')
   rs=b['rs']if isinstance(b['rs'],list)else[b['rs']];assert rs and all((Path('sdk-rs')/r.split('::')[0]).is_file()for r in rs)
   notes.append(b['note']);targets+=rs;moved.append({'ts':b['ts'],'before':a['status'],'after':b['status'],'rs':b['rs']})
assert sum(before.values())==534 and sum(after.values())==534 and {r['ts']for r in moved}==expected and len(set(notes))==10 and len(set(targets))==15
remaining={n:sum(r['status']=='pending'for r in tomllib.loads((root/'exchanges/woo'/f'woo.v3.{n}.ts.toml').read_text())['symbol'])for n in ['algo-writes','account-writes','cancel-all-after']}
assert all(n==0 for n in remaining.values())
print(json.dumps({'sha':sha,'parent':parent,'manifest_files':22,'rows':534,'before':dict(before),'after':dict(after),'rows_moved':10,'ported':0,'divergent':10,'distinct_notes':10,'distinct_targets':15,'remaining_assigned_files':remaining,'moved':moved},indent=2))
