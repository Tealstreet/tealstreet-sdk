# Run from the repository root, with no concurrent Cargo or parity jobs.
# Each mutation is restored in finally; assertion controls reject compile-only failures.
from pathlib import Path
import subprocess,os,json
root=Path.cwd();env=dict(os.environ);env['PATH']='/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:'+env['PATH'];sdk=root/'sdk-rs';src=sdk/'safe-cex-transport/src/socket.rs';manifest=sdk/'parity/manifest/http-over-websocket/RelayWebSocket.ts.toml'
cases=[
('fmt',src,lambda s:s+'\nfn deliberately_unformatted( ) { }\n',['cargo','fmt','--all','--check'],sdk),
('clippy',src,lambda s:s.replace('    pub fn headers(&self)', '    pub fn headers(&self)',1).replace('        &self.headers','        let unused_clippy_control = 1;\n        &self.headers',1),['cargo','clippy','-p','safe-cex-transport','--all-targets'],sdk),
('tests',sdk/'safe-cex-transport/src/codec.rs',lambda s:s.replace('if &bytes[..4] != b"TSB1" {','if false {',1),['cargo','test','-p','safe-cex-transport','--all-targets','--','--include-ignored'],sdk),
('wasm',src,lambda s:s+'\n#[cfg(target_arch = "wasm32")]\ncompile_error!("negative wasm gate");\n',['cargo','build','-p','safe-cex-transport','--target','wasm32-unknown-unknown'],sdk),
('extract',manifest,lambda s:s.replace('ts = "RelayWebSocket.constructor"','ts = "RelayWebSocket.constructorMissingControl"',1),['yarn','parity:extract','--check'],root),
('status',manifest,lambda s:s.replace('layer = "adapted"','layer = ""',1),['yarn','parity:check'],root),
('invention',src,lambda s:s+'\npub fn unmapped_how_control() {}\n',['yarn','parity:check'],root),
('report',src,lambda s:s+'\npub fn unmapped_how_control() {}\n',['yarn','parity:report'],root),
('json-root',sdk/'parity/extract-ts/src/extract.ts',lambda s:s.replace("  { ts: 'packages/json-parse/src/', manifest: 'json-parse/', package: 'packages/json-parse/' },\n",'',1),['node','--import','tsx','--test','--test-name-pattern=json-parse root','sdk-rs/parity/extract-ts/test/extract.test.ts'],root),
]
expected = {'fmt':'Diff in', 'clippy':'unused variable', 'tests':'test result: FAILED', 'wasm':'negative wasm gate', 'extract':'orphaned symbols', 'status':'nonempty rs requires layer', 'invention':'unmapped_how_control: public item', 'report':'parity report refused', 'json-root':'ERR_ASSERTION'}
results=[]
for name,file,mutate,cmd,cwd in cases:
 original=file.read_text();changed=mutate(original);assert changed!=original;file.write_text(changed)
 try:
  p=subprocess.run(cmd,cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path('/tmp/p2b-gate-negative-'+name+'.log').write_text(p.stdout)
  print(name,p.returncode,flush=True);results.append({'gate':name,'exit':p.returncode})
  assert p.returncode != 0 and expected[name] in p.stdout, p.stdout[-2000:]
  if name == 'status': assert 'invention:' not in p.stdout
 finally:file.write_text(original)
Path('/tmp/p2b-gate-controls-results.json').write_text(json.dumps(results,indent=2));assert all(r['exit']!=0 for r in results)
