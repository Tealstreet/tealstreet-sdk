from pathlib import Path
import tempfile,re,shutil,subprocess
workspace=Path('sdk-rs').resolve();crate=workspace/'safe-cex-venue-woo'
with tempfile.TemporaryDirectory(prefix='woo-target-audit-',dir=workspace/'target') as d:
 scope=Path(d)
 for dependency in workspace.iterdir():
  if dependency.is_dir() and (dependency/'Cargo.toml').is_file():(scope/dependency.name).symlink_to(dependency,target_is_directory=True)
 config,n=re.subn(r'(?ms)^members = \[.*?^\]','members = ["safe-cex-venue-woo"]',(workspace/'Cargo.toml').read_text(),count=1);assert n==1
 (scope/'Cargo.toml').write_text(config);(scope/'exempt.toml').write_text('')
 manifests=scope/'manifests';manifests.mkdir()
 for p in (workspace/'parity/manifest/exchanges/woo').glob('*.toml'):shutil.copyfile(p,manifests/p.name)
 p=workspace/'parity/manifest/journal/woo.position-adapter.ts.toml';shutil.copyfile(p,manifests/p.name)
 run=subprocess.run([str(workspace/'target/debug/parity-check'),'--workspace',str(scope),'--manifest',str(manifests),'--exempt',str(scope/'exempt.toml')])
 raise SystemExit(run.returncode)
