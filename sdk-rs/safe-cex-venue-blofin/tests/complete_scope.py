"""Canonical TS declaration coverage and existing Rust parity binary, Blofin only; no Cargo."""
from pathlib import Path
import json,tomllib,subprocess,tempfile,re,sys,collections
root=Path(__file__).resolve().parents[3];sdk=root/'sdk-rs';manifests=sdk/'parity/manifest/exchanges/blofin'
coverage=json.loads(subprocess.check_output(['node',str(Path(__file__).with_suffix('.cjs'))],cwd=root,text=True))
for filename,floor in [('blofin.exchange.ts',66),('blofin.ws-private.ts',14)]:
 actual=coverage[filename];assert len(actual)==floor,(filename,'independent declaration floor')
 mapped=tomllib.loads((manifests/(filename+'.toml')).read_text())['symbol']
 assert actual==[{'ts':r['ts'],'kind':r['kind']}for r in mapped],filename+'_source_drift'
 print(f'{filename}: {floor} declarations, zero drift',flush=True)
files=list(manifests.glob('*.toml'));assert len(files)==7,'independent7manifest floor'
counts=collections.Counter(r['status']for file in files for r in tomllib.loads(file.read_text())['symbol']);print('Blofin census:',dict(counts),'stopped at7 files',flush=True)
with tempfile.TemporaryDirectory(prefix='blofin-complete-parity-',dir=sdk/'target')as folder:
 scope=Path(folder);config=(sdk/'Cargo.toml').read_text();config=re.sub(r'members = \[.*?\]','members = ["safe-cex-venue-blofin"]',config,count=1,flags=re.S);(scope/'Cargo.toml').write_text(config)
 for directory in sdk.iterdir():
  if directory.is_dir() and (directory/'Cargo.toml').is_file():
   (scope/directory.name).symlink_to(directory,target_is_directory=True)
 (scope/'exempt.toml').write_text('')
 result=subprocess.run([str(sdk/'target/debug/parity-check'),'--workspace',str(scope),'--manifest',str(manifests),'--exempt',str(scope/'exempt.toml')],cwd=root)
 print('Scoped existing Rust-parity binary exit',result.returncode,flush=True);sys.exit(result.returncode)
