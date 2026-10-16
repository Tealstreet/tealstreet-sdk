from pathlib import Path
from collections import Counter
import json,subprocess,sys,hashlib
import tomllib
p=Path(__file__).resolve().parent;root=p.parents[3]
base='d4e20845a85cc4e2711c99648ef7547f8aa806df';source='1509a7993843aa4aa4df247fd32bfa80f43bb15e'
selection=json.loads((p/'selection.json').read_text());audit=json.loads((p/'classification.json').read_text())
assert selection['input_sha']==audit['frozen_input_sha']==base
assert audit['source_input_sha']==source
expected={'mexc':(40,{'ported':145,'divergent':40},{'supported':2,'finding':31,'unresolved':7},{'explanation':9,'scheduling':4,'documentation':2,'existing':16}), 'asterdex':(92,{'ported':130,'divergent':92,'pending':9},{'supported':36,'finding':54,'unresolved':2},{'explanation':20,'documentation':15,'existing':19})}
changed=[]
for venue,(population,census,outcomes,types) in expected.items():
 prefix='sdk-rs/parity/manifest/exchanges/'+venue
 paths=subprocess.check_output(['git','ls-tree','-r','--name-only',base,'--',prefix],cwd=root,text=True).splitlines()
 assert len(paths)==(6 if venue=='mexc' else 8),'AUDIT pinned source file floor'
 frozen=[];counts=Counter()
 for file in paths:
  old=tomllib.loads(subprocess.check_output(['git','show',base+':'+file],cwd=root,text=True));current=tomllib.loads((root/file).read_text())
  if '--control-status' in sys.argv and venue=='mexc' and file==paths[0]:current['symbol'][0]['status']='__fault__'
  assert old['file']==current['file'],'AUDIT file targets changed'
  assert len(old['symbol'])==len(current['symbol']),'AUDIT row added/lost'
  for a,b in zip(old['symbol'],current['symbol']):
   assert {k:v for k,v in a.items() if k!='note'}=={k:v for k,v in b.items() if k!='note'},'AUDIT-ZERO-STATUS target/status/identity changed'
   counts[b['status']]+=1
   if a.get('note')!=b.get('note'):
    assert b['note'].startswith('FINDING typed divergence audit (owner: rust-venue-polymarket):')
    assert b['note'].endswith('Historical note follows: '+a['note']),'AUDIT history erased'
    changed.append((venue,a['ts']))
   if a['status']=='divergent':frozen.append({'manifest':file,'source':old['file']['ts'],**a})
 frozen.sort(key=lambda r:(r['source'],r['ts']))
 assert len(frozen)==population and frozen==selection['venues'][venue]['rows'],'AUDIT immutable full selection'
 assert counts==census==selection['venues'][venue]['triple'],'AUDIT independent venue census'
 rows=audit['venues'][venue]['rows'];assert len(rows)==population
 for i,(a,b) in enumerate(zip(frozen,rows),1):
  assert (a['source'],a['ts'],a['rs'],a['manifest'],i)==(b['source'],b['ts'],b['rs'],b['manifest'],b['iteration']),'AUDIT verdict substitution'
 if '--control-type' in sys.argv and venue=='mexc':rows[0]['cause_class']='behavior'
 assert Counter(r['classification'] for r in rows)==outcomes
 assert Counter(r['cause_class'] for r in rows if r['classification']=='finding')==types,'AUDIT-PRIMARY-TYPES changed'
 assert audit['venues'][venue]['counts']==outcomes and audit['venues'][venue]['finding_types']==types
 for tree in ['packages/safe-cex/src/exchanges/'+venue,'sdk-rs/safe-cex-venue-'+venue]:
  assert not subprocess.check_output(['git','diff',base,'--',tree],cwd=root),'AUDIT production/native-test change'
assert len(changed)==53 and set(changed)=={(v,ts) for v,i,ts in audit['new_note_annotations']},'AUDIT independent annotation floor'
assert audit['new_note_annotation_count']==53 and audit['status_rows_moved']==0
assert audit['totals']=={'supported':38,'finding':85,'unresolved':9}
assert audit['finding_types']=={'explanation':29,'scheduling':4,'documentation':17,'existing':35}
assert audit['requested_floor']=={'mexc':40,'asterdex':33}
for name in ['source-core.mjs','source-observations.mjs','aster-source-observations.mjs','run-controls.py','run-aster-controls.py']:
 relative=str((p/name).relative_to(root));assert subprocess.check_output(['git','show',source+':'+relative],cwd=root)==(p/name).read_bytes(),'AUDIT measured source changed'
for relative,n,last in [('measured-1509a79938',15,'mexc-query-encoding'),('aster-measured-1509a79938',4,'aster-public-message-consumer')]:
 r=json.loads((p/relative/'controls.json').read_text());assert r['input_sha']==source and r['accepted'] and r['tracked_inputs_unchanged']
 assert len(r['controls'])==n and r['stop']==last
 for item in [r['all_source_observations']]+[c[stage] for c in r['controls'] for stage in ['baseline','mutant','restored']]:
  assert hashlib.sha256((p/relative/item['log']).read_bytes()).hexdigest()==item['log_sha256'],'AUDIT source log hash'
 assert r['all_source_observations']['exit']==0
 for c in r['controls']:
  assert c['caught'] and [c[s]['exit'] for s in ['baseline','mutant','restored']]==[0,1,0]
  text=(p/relative/c['mutant']['log']).read_text();assert 'AssertionError' in text and c['marker'] in text,'AUDIT named source failure'
r=json.loads((p/'measured-9415fc16f8/controls.json').read_text());assert not r['accepted'] and len(r['controls'])==15
assert [c['profile'] for c in r['controls'] if not c['caught']]==['mexc-fetchTickers'],'AUDIT rejected control finding lost'
assert not audit['checker_screen']['exact_public_types_star_suspects']
assert audit['checker_screen']['private_glob_analogues']==['safe-cex-venue-asterdex/src/account_fetch.rs::AccountFetch','safe-cex-venue-asterdex/src/order_execution.rs::OrderExecution']
print(json.dumps({'input_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'frozen_input_sha':base,'triple':{'mexc':[145,40,0],'asterdex':[130,92,9]},'population':132,'supported':38,'finding':85,'unresolved':9,'finding_types':audit['finding_types'],'annotation_count':53,'status_rows_moved':0,'source_controls':19,'stop':'mexc40/asterdex92'}))
