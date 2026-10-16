"""Nine-file tomllib inventory at explicit SHAs; census is separate from parity proof."""
import collections,json,subprocess,sys,tomllib
base=subprocess.check_output(['git','rev-parse',sys.argv[1]],text=True).strip()
head=subprocess.check_output(['git','rev-parse',sys.argv[2]],text=True).strip()
names=['accessToken.ts','config/config.ts','index.ts','exchangeMetadata.ts','createExchangeFromAccount.ts','telemetry.ts','utils/venueRestDiagnostics.ts','utils/wsDiagnostics.ts','instrument-class/resolve.ts']
expected_pending=[12,0,8,8,2,0,26,9,0] # user assignment and landed PR7437 inventory
statuses=['ported','divergent','pending'];before=collections.Counter();after=collections.Counter();files=[];moves=collections.Counter()
for name,expected in zip(names,expected_pending,strict=True):
 path='sdk-rs/parity/manifest/'+name+'.toml'
 def read(sha):return tomllib.loads(subprocess.check_output(['git','show',sha+':'+path],text=True))['symbol']
 old,new=read(base),read(head);a=collections.Counter(row['status'] for row in old);b=collections.Counter(row['status'] for row in new);assert a['pending']==expected,(name,a,expected)
 old_by_id={(row['ts'],row['kind']):row for row in old};new_by_id={(row['ts'],row['kind']):row for row in new};assert old_by_id.keys()==new_by_id.keys(),name
 for key,row in new_by_id.items():
  if old_by_id[key]['status']=='pending' and row['status']!='pending':moves[row['status']]+=1;assert row['rs'] and row['layer'] and row['note'],(name,key)
  if row['status']=='divergent':assert row['note'].startswith(('FORCED','CHOSEN','MIXED','CORRECTED','FINDING')),(name,key)
 before.update(a);after.update(b);files.append({'file':name,'before':[a[s] for s in statuses],'after':[b[s] for s in statuses]})
assert [before[s] for s in statuses]==[24,6,65],before
assert sum(before.values())==95 and sum(after.values())==95
assert sum(moves.values())==65,moves
print(json.dumps({'base':base,'sha':head,'order':statuses,'before':[before[s] for s in statuses],'after':[after[s] for s in statuses],'rows_moved':sum(moves.values()),'movement':dict(moves),'files':files,'stopped_after_file':9,'scope':'nine assigned source files, 95 symbols; census only, not completeness proof'},indent=2))
