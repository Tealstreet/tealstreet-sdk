"""Serial, SHA-attached scoped gates. Do not run alongside another Cargo command."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1];root=crate.parent.parent
logs=Path(os.environ['POLYMARKET_AUTH_INIT_GATE_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket'),str(root/'packages/safe-cex/src/exchanges/polymarket')],text=True).strip(),'commit all inputs before gates'
cargo='/opt/homebrew/opt/rustup/bin/cargo';yarn='/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js'
base=['--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-polymarket'];locked=['--locked','--jobs','2']
gates=[('source-oracle',['node','sdk-rs/safe-cex-venue-polymarket/tests/auth_init_oracle.cjs']),('check',[cargo,'check',*base,'--all-targets',*locked]),('tests',[cargo,'test',*base,*locked]),('fmt',[cargo,'fmt',*base,'--','--check']),('targets',['python3.14','sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py']),('writer',['node',yarn,'parity:extract']),('extractor',['node',yarn,'parity:extract:check'])]
focused=os.environ.get('POLYMARKET_AUTH_INIT_FOCUSED')=='1'
if focused:
 gates=[(name,cmd+['--test','auth_init'] if name=='tests' else cmd) for name,cmd in gates]
sheet={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'focused_auth_init':focused,'gates':[]}
for name,cmd in gates:
 with (logs/(name+('.json' if name=='source-oracle' else '.log'))).open('w') as out,(logs/(name+'.err')).open('w') as err:r=subprocess.run(cmd,cwd=root,stdout=out,stderr=err)
 row={'name':name,'command':cmd,'exit':r.returncode};sheet['gates'].append(row);(logs/'gates.json').write_text(json.dumps(sheet,indent=2)+'\n');assert r.returncode==0,(name,r.returncode)
 if name=='source-oracle':
  data=json.loads((logs/'source-oracle.json').read_text());assert len(data['cases'])==96 and data['cases'][95]['name']=='publicConstructor' and data['cases'][95]['i']==11;row['source_fixture_floor']=96;row['stop']='publicConstructor/11'
 if name=='tests':
  passed=sum(int(x) for x in re.findall(r'test result: ok\. (\d+) passed; 0 failed;', (logs/'tests.log').read_text()));assert passed==(8 if focused else 119),passed;row['passed_floor']=8 if focused else 119
 if name in ['writer','extractor']:
  expected='Updated 0 manifests; 0 added, 0 orphaned symbols.' if name=='writer' else 'Would update 0 manifests; 0 added, 0 orphaned symbols.';assert expected in (logs/(name+'.log')).read_text(),name;row['output_confirmed']=expected
 (logs/'gates.json').write_text(json.dumps(sheet,indent=2)+'\n');print(name+': exit '+str(r.returncode),flush=True)
