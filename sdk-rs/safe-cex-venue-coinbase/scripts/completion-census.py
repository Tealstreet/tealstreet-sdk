"""Committed tomllib Coinbase census and independent real-target inspection."""
import collections,json,pathlib,re,subprocess,sys,tomllib
base=sys.argv[1];sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
files=sorted(pathlib.Path('sdk-rs/parity/manifest/exchanges/coinbase').glob('*.toml'));assert len(files)==16

def rows(rev):
 return {(str(p),s['ts']):s for p in files for s in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(p)],text=True))['symbol']}
before,after=rows(base),rows(sha);assert len(before)==len(after)==426
moved=[]
for key,row in after.items():
 if before[key]['status']=='pending' and row['status']!='pending':
  targets=row['rs'] if isinstance(row['rs'],list) else [row['rs']]
  assert targets and all(targets)
  verified=[]
  for target in targets:
   path,item=target.split('::',1);source=subprocess.check_output(['git','show',sha+':sdk-rs/'+path],text=True)
   name=item.split('::')[-1];kind='struct' if '::' not in item and row['kind']=='class' else 'fn'
   assert re.search(r'pub '+kind+r' '+re.escape(name)+r'\b',source),(key,target,kind)
   if '::' in item: assert re.search(r'impl[^\n]*\b'+re.escape(item.split('::')[0])+r'\b',source),(key,target)
   verified.append(dict(rs=target,actual_native_kind=kind))
  if row['status']=='divergent':assert row['note'].startswith('FINDING:')
  moved.append(dict(ts=key[1],status=row['status'],verified_targets=verified))
counts=lambda rs:dict(collections.Counter(r['status'] for r in rs.values()))
print(json.dumps(dict(sha=sha,base=base,iteration='Stopped after 16 Coinbase manifests / 426 declared rows and every newly off-pending native target. No other venue census.',before=counts(before),after=counts(after),moved=moved,moved_ported=sum(v['status']=='ported' for v in moved),moved_divergent=sum(v['status']=='divergent' for v in moved),proof_limit='Census and actual fn/struct inspection are distinct from full behavior proof. Separately run unchanged scoped AST validator. Four real class targets remain divergent FINDING; zero pending is not whole-venue parity.'),indent=2))
