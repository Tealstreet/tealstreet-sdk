"""Splice source arrays, then restore byte-exactly; one Node per scoped corpus."""
import json, os, pathlib, subprocess, sys
ROOT=pathlib.Path(__file__).resolve().parents[3]
TESTS=ROOT/'sdk-rs/safe-cex-venue-blofin/tests'
MODE=sys.argv[1] if len(sys.argv)>1 else 'baseline'
logs=ROOT/f'sdk-rs/target/blofin-cardinality-source-{MODE}'
logs.mkdir(exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
sites=[(p.stem.removesuffix('_oracle'),'cases') for p in sorted(TESTS.glob('*_oracle.cjs'))]
sites += [('orders','aliases'),('inline_tpsl','infoAliases')]
if len(sys.argv)>2: sites=[v for v in sites if v==tuple(sys.argv[2].split(':'))]
summary=[]
for target,field in sites:
 path=TESTS/f'{target}_oracle.cjs';original=path.read_bytes()
 for count,label in [(0,'empty'),(1,'subset')]:
  # For cases retain all but one for subset; for nested collections keep first.
  operation=f'c.cases.splice({0 if count==0 else "c.cases.length-1"});' if field=='cases' else f'for(const v of c.cases)if(Array.isArray(v.{field}))v.{field}.splice({count});'
  if target=='orders' and field=='aliases' and count==1:operation='c.cases.find(v=>Array.isArray(v.aliases)&&v.aliases.length).aliases.pop();'
  hook='const cardinalityWrite=process.stdout.write.bind(process.stdout);process.stdout.write=(text)=>{const c=JSON.parse(text);const before=JSON.stringify(c);'+operation+'if(JSON.stringify(c)===before)throw new Error("cardinality_source_control_no_change");return cardinalityWrite(JSON.stringify(c));};\n'
  command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-blofin','--locked','--jobs','1','--test',target,'--','--nocapture']
  try:
   path.write_bytes(hook.encode()+original)
   r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);output=r.stdout+r.stderr
   (logs/f'{target}.{field}.{label}.mutated.log').write_text(output)
   if r.returncode==0:verdict='SURVIVED'
   elif r.returncode==101 and 'panicked at' in output and 'test result: FAILED' in output:verdict='RUNTIME_REJECTED'
   else:raise AssertionError(output)
   if not MODE.startswith('baseline'):assert 'BLOFIN-CARDINALITY source count' in output,output
   summary.append({'target':target,'field':field,'mutation':label,'verdict':verdict,'exit':r.returncode})
   print(target,field,label,verdict,r.returncode,flush=True)
  finally:path.write_bytes(original)
  r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
  (logs/f'{target}.{field}.{label}.restored.log').write_text(r.stdout+r.stderr)
  assert r.returncode==0,r.stdout+r.stderr
  assert path.read_bytes()==original
  (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
if not MODE.startswith('baseline'):assert all(s['verdict']=='RUNTIME_REJECTED' for s in summary)
