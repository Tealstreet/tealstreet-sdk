from pathlib import Path
from collections import Counter
import json,subprocess,sys,re
p=Path(__file__).resolve().parent;root=p.parents[3]
r=json.loads((p/'classification.json').read_text())
assert r['input_sha']=='d4e20845a85cc4e2711c99648ef7547f8aa806df'
old=r['orphan_15']['rows'];sample=r['original_36']['rows'];latest=r['latest_110']['rows']
assert len(old)==15 and len(sample)==36 and len(latest)==110
if '--control-type' in sys.argv:old[0]['primary_type']='explanation'
assert Counter(x['primary_type'] for x in old)=={'behavior':6,'explanation':8,'documentation':1},'EARLIER-TYPES historical primary types changed'
assert {x['ts'] for x in old if x['primary_type']=='behavior'}=={'CandleCacheChunkRecord','initBroadcastChannel','bybitInstrumentClass','OKXExchange.findOrderByClientOrderId','okxBillCategory','mapOkxBillToIncomeRecords'}
assert Counter(x['classification'] for x in sample)=={'supported':16,'finding':19,'unresolved':1}
assert Counter(x['cause_class'] for x in sample if x['classification']=='finding')=={'explanation':16,'scheduling':1,'documentation':1,'existing':1}
assert sample==[x for x in latest if x.get('in_original_sample')]
assert Counter(x['classification'] for x in latest)=={'supported':48,'finding':58,'unresolved':4}
assert Counter(x['cause_class'] for x in latest if x['classification']=='finding')=={'explanation':40,'scheduling':11,'documentation':1,'existing':4,'behavior':2}
assert len({(x['source'],x['ts']) for x in latest})==110
source='sdk-rs/parity/evidence/okx-kraken-weex-audit/full-classification.json'
full=json.loads(subprocess.check_output(['git','show','b447f40829:'+source],cwd=root,text=True))
poly=json.loads(subprocess.check_output(['git','show','789e22bfac:sdk-rs/parity/evidence/polymarket-divergence-audit/classification.json'],cwd=root,text=True))
assert [(x['source'],x['ts'],x['classification'],x['cause_class']) for x in latest]==[(x['source'],x['ts'],x['classification'],x['cause_class']) for group in full['venues'].values() for x in group['rows']]+[(x['source'],x['ts'],x['classification'],x['cause_class']) for x in poly['rows']]
scan=r['checker_shape_scan']['targets'];expected=[]
for row in latest:
    if row['classification']!='finding':continue
    for target in ([row['rs']] if isinstance(row['rs'],str) else row['rs']):
        expected.append((row['source'],row['ts'],target))
assert [(x['source'],x['ts'],x['target']) for x in scan]==expected
for x in scan:
    text=(root/x['file']).read_text();name=x['target'].split('::')[-1]
    glob=bool(re.search(r'pub\s+use\s+(?:[\w:]+::)?types::\*\s*;',text))
    local=bool(re.search(r'\b(?:struct|enum)\s+'+re.escape(name)+r'\b',text))
    assert (glob,local,glob and local)==(x['types_star_same_file'],x['target_is_local_struct_or_enum'],x['suspect_shape'])
assert not r['checker_shape_scan']['suspect_rows']
for tree in ['sdk-rs/parity/manifest','packages/safe-cex/src','sdk-rs/safe-cex-venue-polymarket','sdk-rs/safe-cex-venue-okx','sdk-rs/safe-cex-venue-kraken','sdk-rs/safe-cex-venue-weex']:
    assert not subprocess.check_output(['git','diff',r['input_sha'],'--',tree],cwd=root),'EARLIER-TYPES source/manifest changed'
assert r['status_rows_moved']==0 and not r['new_runtime_gates']
print(json.dumps({'input_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'historical_15_types':{'behavior':6,'explanation':8,'documentation':1},'original36_subsumed':True,'latest110_types':{'explanation':40,'scheduling':11,'documentation':1,'existing':4,'behavior':2},'checker_suspect_rows':0,'status_rows_moved':0}))
