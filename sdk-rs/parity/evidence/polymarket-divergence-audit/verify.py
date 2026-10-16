"""Frozen venue census, zero-status audit, and source-control receipt integrity."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import subprocess
import sys
import tomllib

directory=Path(__file__).resolve().parent
root=directory.parents[3]
base='fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c'
integration='333fd419fdbf70f081039610f22da9ee653bc96f'
source_input='a6f625951421abc4aa5319ce94784024227646b5'
selection=json.loads((directory/'selection.json').read_text())
verdicts=json.loads((directory/'classification.json').read_text())
assert selection['input_sha']==verdicts['frozen_input_sha']==base
assert verdicts['integration_parent']==integration
assert verdicts['source_control_input_sha']==source_input
prefix='sdk-rs/parity/manifest/exchanges/polymarket'
paths=subprocess.check_output(['git','ls-tree','-r','--name-only',base,'--',prefix],cwd=root,text=True).splitlines()
assert len(paths)==7,'POLY-AUDIT independent source-file floor'
frozen=[]; census=Counter(); annotated=[]
for file in paths:
    before=tomllib.loads(subprocess.check_output(['git','show',f'{base}:{file}'],cwd=root,text=True))
    parent=tomllib.loads(subprocess.check_output(['git','show',f'{integration}:{file}'],cwd=root,text=True))
    current=tomllib.loads((root/file).read_text())
    assert before==parent,'POLY-AUDIT frozen/integration manifests differ'
    if '--control-status' in sys.argv and file==paths[0]:
        current['symbol'][0]['status']='__deliberate_status_fault__'
    assert before['file']==current['file'],'POLY-AUDIT file targets changed'
    assert len(before['symbol'])==len(current['symbol']),'POLY-AUDIT rows added/lost'
    for a,b in zip(before['symbol'],current['symbol']):
        assert {k:v for k,v in a.items() if k!='note'}=={k:v for k,v in b.items() if k!='note'},'POLY-AUDIT-ZERO-STATUS targets/status/identity changed'
        census[b['status']]+=1
        if a.get('note')!=b.get('note'):
            assert b['note'].startswith('FINDING Polymarket divergence audit (owner: rust-venue-polymarket):'),'POLY-AUDIT finding prefix'
            assert b['note'].endswith('Historical note follows: '+a['note']),'POLY-AUDIT historical note lost'
            annotated.append((before['file']['ts'],a['ts']))
        if a['status']=='divergent':
            frozen.append({'manifest':file,'source':before['file']['ts'],**a})
frozen.sort(key=lambda r:(r['source'],r['ts']))
assert len(frozen)==18 and frozen==selection['rows'],'POLY-AUDIT immutable eighteen-row selection'
assert census=={'ported':107,'divergent':18,'pending':16},'POLY-AUDIT independent venue census'
assert selection['triple']==dict(census)
rows=verdicts['rows']
assert len(rows)==18 and verdicts['selection_stop']==18 and verdicts['selection_last']=='PolymarketPrice'
for i,(a,b) in enumerate(zip(frozen,rows),1):
    assert (a['source'],a['ts'],a['rs'],i)==(b['source'],b['ts'],b['rs'],b['iteration']),'POLY-AUDIT verdict substitution'
assert Counter(r['classification'] for r in rows)=={'supported':1,'finding':17}
assert verdicts['totals']=={'supported':1,'finding':17,'unresolved':0}
classes=Counter(r['cause_class'] for r in rows if r['classification']=='finding')
assert classes==verdicts['finding_classes']=={'scheduling':5,'explanation':12}
assert len(annotated)==17 and set(annotated)=={(r['source'],r['ts']) for r in rows if r['classification']=='finding'}
assert verdicts['status_rows_moved']==0 and verdicts['new_finding_annotations']==17
for tree in ['packages/safe-cex/src/exchanges/polymarket','sdk-rs/safe-cex-venue-polymarket']:
    assert not subprocess.check_output(['git','diff',base,'--',tree],cwd=root),'POLY-AUDIT production/native-test change from frozen input'
receipt=json.loads((directory/'measured-a6f6259514/controls.json').read_text())
assert receipt['input_sha']==source_input and receipt['accepted'] and receipt['tracked_inputs_unchanged']
assert len(receipt['controls'])==18 and receipt['stop']=='PolymarketL2Headers'
assert {c['profile'] for c in receipt['controls']}=={r['profile'] for r in rows},'POLY-AUDIT profile identities'
for name in ['source-observations.mjs','run-controls.py']:
    relative=str((directory/name).relative_to(root))
    assert subprocess.check_output(['git','show',f'{source_input}:{relative}'],cwd=root)==(directory/name).read_bytes(),'POLY-AUDIT measured source input drift'
for item in [receipt['all_source_observations']]+[c[stage] for c in receipt['controls'] for stage in ['baseline','mutant','restored']]:
    log=directory/'measured-a6f6259514'/item['log']
    assert hashlib.sha256(log.read_bytes()).hexdigest()==item['log_sha256'],'POLY-AUDIT log hash'
assert receipt['all_source_observations']['exit']==0
for c in receipt['controls']:
    assert c['caught'] and [c[s]['exit'] for s in ['baseline','mutant','restored']]==[0,1,0]
    log=(directory/'measured-a6f6259514'/c['mutant']['log']).read_text()
    assert 'AssertionError' in log and c['marker'] in log,'POLY-AUDIT named runtime assertion'
print(json.dumps({'input_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'frozen_sha':base,'integration_parent':integration,'triple':dict(census),'supported':1,'finding':17,'unresolved':0,'finding_classes':dict(classes),'new_finding_annotations':17,'status_rows_moved':0,'source_profile_controls':18,'last_manifest_iteration':'18/PolymarketPrice'}))
