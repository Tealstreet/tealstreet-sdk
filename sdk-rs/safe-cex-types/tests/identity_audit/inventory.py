from pathlib import Path
import json,subprocess,itertools,collections
roots=[Path('sdk-rs/safe-cex-store/tests'),Path('sdk-rs/safe-cex-types/tests'),Path('sdk-rs/safe-cex-store/src'),Path('sdk-rs/safe-cex-types/src')]
files=[p for r in roots for p in r.rglob('*.json')]
providers=['sdk-rs/safe-cex-store/tests/derived_oracle.mjs','sdk-rs/safe-cex-store/tests/p4_store_oracle.mjs','sdk-rs/safe-cex-types/tests/p4_model_oracle.cjs','sdk-rs/safe-cex-types/tests/p4_accounts_oracle.cjs','sdk-rs/safe-cex-types/tests/query_value.cjs','sdk-rs/safe-cex-types/tests/validation-error.cjs','sdk-rs/safe-cex/tests/position_record_numbers_oracle.mjs','sdk-rs/safe-cex-num/tests/get_key.cjs','sdk-rs/safe-cex-store/tests/native_patch_oracle.mjs','sdk-rs/safe-cex-store/tests/p4_orderbook_oracle.cjs','sdk-rs/safe-cex-store/tests/sequenced_book_oracle.mjs','sdk-rs/safe-cex/tests/risk_recovery_oracle.mjs','sdk-rs/safe-cex/tests/dictionary_write_oracle.mjs']
providers += ['sdk-rs/safe-cex-types/tests/p4_symbols_oracle.cjs','sdk-rs/safe-cex-types/tests/p4_constants_oracle.cjs','sdk-rs/safe-cex-types/tests/strict_wire_oracle.cjs','sdk-rs/safe-cex-types/tests/position_record_oracle.cjs','sdk-rs/safe-cex-store/tests/notification_oracle.mjs','sdk-rs/safe-cex-store/tests/risk_identity_supplement.mjs']
counts={};pairs=collections.Counter();samples=[];distinct_pairs=collections.Counter();hex_keys=collections.Counter()
def walk(x,path):
 if isinstance(x,dict):
  counts['records']+=1;groups=collections.defaultdict(list)
  for k,v in x.items():
   if isinstance(v,str) and len(v)==16 and all(c in '0123456789abcdefABCDEF' for c in v):
    try:
     bits=int(v,16);counts['hex64_candidates']+=1;hex_keys[k]+=1
     if k in ['bits','$bits']:counts['bit_captures']+=1
     if bits&0x7ff0000000000000==0x7ff0000000000000 and bits&0xfffffffffffff:counts['raw_nan_bits']+=1
    except ValueError:pass
   if k in ['$number','number'] and v=='NaN':counts['stable_nan_tags']+=1
   if v is None or isinstance(v,(dict,list)):continue
   groups[(type(v).__name__,json.dumps(v,sort_keys=True))].append(k)
  scalar=[(k,v) for k,v in x.items() if isinstance(v,(bool,str,int,float))]
  for (a,av),(b,bv) in itertools.combinations(scalar,2):
   if type(av)==type(bv) and av!=bv:
    distinct_pairs[(type(av).__name__,*sorted((a,b)))]+=1
  for (typ,val),ks in groups.items():
   if len(ks)>1:
    counts['equal_groups']+=1
    for a,b in itertools.combinations(ks,2):pairs[(typ,a,b)]+=1
    if len(samples)<100:samples.append({'path':path,'type':typ,'value':val,'fields':ks})
  for k,v in x.items():walk(v,path+'.'+k)
 elif isinstance(x,list):
  for i,v in enumerate(x):walk(v,path+f'[{i}]')
counts=collections.Counter();fail=[]
for p in files:
 walk(json.loads(p.read_text()),str(p));counts['frozen_files']+=1
for p in providers:
 r=subprocess.run(['node',p],capture_output=True,text=True)
 if r.returncode:fail.append({'file':p,'stderr':r.stderr[-500:]});continue
 try:x=json.loads(r.stdout)
 except Exception:fail.append({'file':p,'error':'nonJSON provider output'});continue
 Path('/tmp/p4-fixture-provider-'+Path(p).stem+'.json').write_text(r.stdout)
 walk(x,p);counts['live_providers']+=1
 if p.endswith('position_record_numbers_oracle.mjs'):
  for i,row in enumerate(x):
   # Raw JSON fixture is itself a record feeding a downstream numeric gate.
   try:walk(json.loads(row['raw']),p+f'.raw[{i}]');counts['numeric_raw_records']+=1
   except Exception:fail.append({'file':p,'error':'raw fixture parse'})
out={'counts':dict(counts),'hex64_keys':dict(hex_keys),'failures':fail,'equal_field_pairs':[{'type':k[0],'left':k[1],'right':k[2],'records':n,'distinct_records':distinct_pairs[(k[0],*sorted(k[1:]))]} for k,n in pairs.most_common()],'samples':samples,'scope':'JSON-visible scalar equality candidates; enums provisionally strings; excludes Rust-native constructed records until separate AST/type audit. Equality is candidate evidence, not proof of absent field-identity cases.'}
Path(__import__('os').environ.get('P4_IDENTITY_AUDIT_OUTPUT', '/tmp/p4-fixture-identity-audit.json')).write_text(json.dumps(out,indent=2));print(json.dumps({'counts':out['counts'],'failures':fail,'top_pairs':out['equal_field_pairs'][:12]},indent=2))

if fail: raise SystemExit("fixture inventory incomplete: provider or raw parse failure")
