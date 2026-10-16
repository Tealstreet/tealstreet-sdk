"""Tomllib census against the requested reviewed integration parent."""
import collections,json,pathlib,re,subprocess,sys,tomllib
base=sys.argv[1];sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
files=sorted(pathlib.Path('sdk-rs/parity/manifest/exchanges/coinbase').glob('*.toml'));assert len(files)==16
rows=lambda rev:{(str(p),s['ts']):s for p in files for s in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(p)],text=True))['symbol']}
before,after=rows(base),rows(sha);assert len(before)==len(after)==426
moved=[]
for key,row in after.items():
 if before[key]['status']=='pending' and row['status']!='pending':
  path,name=row['rs'].split('::');source=subprocess.check_output(['git','show',sha+':sdk-rs/'+path],text=True)
  kind='fn'
  assert re.search(r'pub '+kind+r' '+re.escape(name)+r'\b',source),(key,row['rs'])
  moved.append(dict(ts=key[1],status=row['status'],rs=row['rs'],native_kind=kind))
counts=lambda rows:dict(collections.Counter(s['status'] for s in rows.values()))
print(json.dumps(dict(sha=sha,base=base,iteration='stopped after all 16 Coinbase manifests / 426 declared rows; no other venue census',before=counts(before),after=counts(after),moved=moved,moved_ported=sum(v['status']=='ported' for v in moved),moved_divergent=sum(v['status']=='divergent' for v in moved),target_verification='Every new off-pending row resolves to committed file and actual pub fn; Independently scoped AST parity-check is also required.'),indent=2))
