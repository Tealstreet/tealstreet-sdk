# Run from the repository root, with no concurrent Cargo or parity jobs.
# Each mutation is restored in finally; assertion controls reject compile-only failures.
from pathlib import Path
import subprocess,os,json
root=Path.cwd();sdk=root/'sdk-rs';p=sdk/'safe-cex-transport/src/relay.rs';env=dict(os.environ);env['PATH']='/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:'+env['PATH']; original=p.read_text();results=[]
for name,old,new,cmd,cwd in [
('lazy-preflight','        Self::validate_connection_headers(self.host.as_ref(), &self.connection_options)?;','', ['cargo','test','-p','safe-cex-transport','--test','tunnel','relay_required_auth_is_rechecked_before_lazy_dial'],sdk),
('wasm-ignore','if !cfg!(target_arch = "wasm32")','if true', ['yarn','warning:wasm'],sdk/'safe-cex-transport'),
('wasm-required','(cfg!(target_arch = "wasm32")','(false', ['yarn','warning:wasm'],sdk/'safe-cex-transport')]:
 assert original.count(old)==1,(name,original.count(old));p.write_text(original.replace(old,new,1))
 try:
  r=subprocess.run(cmd,cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);Path('/tmp/p2b-control-'+name+'.log').write_text(r.stdout);print(name,r.returncode,flush=True);results.append({'name':name,'exit':r.returncode})
  if name == 'lazy-preflight': assert r.returncode == 101 and 'test result: FAILED' in r.stdout
  elif name == 'wasm-ignore': assert r.returncode == 1 and 'RuntimeError: unreachable' in r.stdout and 'probe_relay_overlay::{closure#0}' in r.stdout
  else: assert r.returncode == 1 and 'AssertionError [ERR_ASSERTION]' in r.stdout and 'requiredRejected: false' in r.stdout
 finally:p.write_text(original)
Path('/tmp/p2b-overlay-controls-results.json').write_text(json.dumps(results,indent=2))
