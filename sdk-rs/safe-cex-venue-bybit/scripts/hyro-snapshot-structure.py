"""Strict eight-row/new-module scope; no inherited Bybit or dependency body claim."""
from pathlib import Path
import os,re,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[3];SDK=ROOT/'sdk-rs';CRATE=SDK/'safe-cex-venue-bybit'
ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
with tempfile.TemporaryDirectory(prefix='.parity-snapshot-',dir=CRATE) as directory:
 scope=Path(directory);config,n=re.subn(r'(?ms)^members = \[.*?^\]',f'members = ["{CRATE.name}"]',(SDK/'Cargo.toml').read_text(),count=1);assert n==1
 (scope/'Cargo.toml').write_text(config)
 for member in SDK.iterdir():
  if member.is_dir() and (member/'Cargo.toml').is_file() and member!=CRATE:(scope/member.name).symlink_to(member,target_is_directory=True)
 projected=scope/CRATE.name;(projected/'src').mkdir(parents=True);(projected/'Cargo.toml').write_bytes((CRATE/'Cargo.toml').read_bytes())
 exports=re.findall(r'^pub mod hyro_snapshot;$',(CRATE/'src/lib.rs').read_text(),re.M);assert len(exports)==1
 (projected/'src/lib.rs').write_text(exports[0]+'\n');(projected/'src/hyro_snapshot.rs').symlink_to(CRATE/'src/hyro_snapshot.rs')
 manifests=scope/'manifests';manifests.mkdir()
 file=SDK/'parity/manifest/exchanges/bybit/hyrotrader.exchange.ts.toml'
 code="""
import {readFileSync} from 'node:fs';import {parseManifest,serializeManifest} from './sdk-rs/parity/extract-ts/src/manifest.ts';const p=process.argv[1];const m=parseManifest(readFileSync(p,'utf8'),'packages/safe-cex/src/exchanges/bybit/hyrotrader.exchange.ts');const names=['.reconcilePrivateState', '.fetchAndSetPrivateSnapshot', '.fetchOrdersPositionsAndBalance', '.fetchAndSetPrivateSnapshotHelper', '.fetchMarkets', '.getBybitRestBaseUrl', '.getBybitPublicWsUrl', '.getBybitPrivateWsUrl'];m.symbol=m.symbol.filter(r=>names.some(n=>r.ts==='HyrotraderExchange'+n));if(m.symbol.length!==8)throw Error('new row scope');m.file.rust_files=['safe-cex-venue-bybit/src/hyro_snapshot.rs'];process.stdout.write(serializeManifest(m));
"""
 projected_manifest=subprocess.check_output(['node','--import','tsx','--input-type=module','-e',code,str(file)],cwd=ROOT,text=True)
 (manifests/file.name).write_text(projected_manifest)
 (scope/'exempt.toml').write_text('')
 result=subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','run','--manifest-path',str(SDK/'Cargo.toml'),'-p','safe-cex-parity','--bin','parity-check','--jobs','2','--','--workspace',str(scope),'--manifest',str(manifests),'--exempt',str(scope/'exempt.toml')],cwd=ROOT,env=ENV)
 result.check_returncode()
 print('Hyro snapshot strict structure: original export and source bytes, eight rows, all new public items, zero exemptions. Broad Bybit inventory was not rerun and is outside this gate.')
