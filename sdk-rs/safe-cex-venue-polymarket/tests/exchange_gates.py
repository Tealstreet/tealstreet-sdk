"""Serial scoped gates. Logs redact local paths because the SDK is public."""
from pathlib import Path
import gzip, hashlib, json, os, re, shutil, subprocess
crate=Path(__file__).resolve().parents[1];root=crate.parent.parent
logs=Path(os.environ['POLYMARKET_GATE_DIR']);logs.mkdir(parents=True,exist_ok=True)
cargo=os.environ.get('CARGO') or shutil.which('cargo');assert cargo
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate/'src'),str(crate/'tests'),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(),'commit inputs before gates'
yarn=os.environ['YARN_JS'];base=['--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-polymarket'];locked=['--locked','--jobs','1']
gates=[('source-oracle',['node','sdk-rs/safe-cex-venue-polymarket/tests/exchange_oracle.cjs']),('completion-source-oracle',['node','sdk-rs/safe-cex-venue-polymarket/tests/completion_oracle.cjs']),('check',[cargo,'check',*base,'--all-targets',*locked]),('tests',[cargo,'test',*base,'--all-targets',*locked,'--','--include-ignored']),('fmt',[cargo,'fmt',*base,'--','--check']),('host-clippy',[cargo,'clippy',*base,'--all-targets',*locked]),('wasm-clippy',[cargo,'clippy',*base,'--lib','--target','wasm32-unknown-unknown',*locked]),('wasm-build',[cargo,'build',*base,'--lib','--target','wasm32-unknown-unknown',*locked]),('targets',['python3.14','sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py']),('writer',['node',yarn,'parity:extract']),('extractor',['node',yarn,'parity:extract:check']),('parity-check',['node',yarn,'parity:check']),('parity-report',['node',yarn,'parity:report']),('parity-progress',['node',yarn,'parity:progress'])]
sheet={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'logs':'local host paths redacted; raw byte SHA256 retained','gates':[]}
for name,cmd in gates:
 r=subprocess.run(cmd,cwd=root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env={**os.environ,'CARGO_BUILD_JOBS':'1'});raw=r.stdout;out=raw.decode(errors='replace');clean=out.replace(str(root),'<worktree>').replace(str(Path.home()),'<home>');clean=re.sub(r'/opt/homebrew/[^\s)]+','<tool>',clean)
 with gzip.open(logs/(name+'.log.gz'),'wb') as f:f.write(clean.encode())
 command=[Path(x).name if x in [cargo,yarn] else x for x in cmd];row={'name':name,'command':command,'exit':r.returncode,'raw_sha256':hashlib.sha256(raw).hexdigest()};sheet['gates'].append(row)
 if name in ['source-oracle','completion-source-oracle'] and r.returncode==0:
  data=json.loads(out);row['fixture_floor']=len(data['cases']);row['stop']=[data['cases'][-1]['name'],data['cases'][-1]['i']];assert (row['fixture_floor'],row['stop'])==((128,['listen',15]) if name=='source-oracle' else (64,['book',7]))
 if name=='tests':
  row['passed']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed; 0 failed;',out)));row['expected_floor']=138;row['count_confirmed']=row['passed']==138
 if name in ['writer','extractor']:row['output_confirmed']=('Updated 0 manifests; 0 added, 0 orphaned symbols.' if name=='writer' else 'Would update 0 manifests; 0 added, 0 orphaned symbols.') in out
 (logs/'gates.json').write_text(json.dumps(sheet,indent=2)+'\n');print(name+': exit '+str(r.returncode),flush=True)
 # Record a failure, then continue independent gates. Never present running gates as passed.
