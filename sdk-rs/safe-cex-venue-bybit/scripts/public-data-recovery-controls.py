#!/usr/bin/env python3
"""One control per new recovery/read method, run sequentially."""
import json,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src/public_data_recovery.rs'
test=root/'sdk-rs/safe-cex-venue-bybit/tests/public_data_recovery.rs'
original={p:p.read_bytes() for p in [source,test]}
evidence=root/'sdk-rs/safe-cex-venue-bybit/evidence/public-data-recovery-controls.json'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','public_data_recovery','--jobs','2','--','--include-ignored']
controls=[
 ('public open event',source,'if reconnecting() {','if !reconnecting() {','RECOVERY onOpen iteration 0: native event order and arguments'),
 ('clear timer',source,'if !truthy(&id) {','if truthy(&id) {','RECOVERY clear iteration 8: native event order and arguments'),
 ('arm timer',source,'set_timeout(callback, 25000.)','set_timeout(callback, 25001.)','RECOVERY arm iteration 16: native event order and arguments'),
 ('existing timer topics',source,"topic.rsplit('.').next()","topic.split('.').next()",'RECOVERY existing iteration 25: native event order and arguments'),
 ('recovery wrapper',source,'reason.unwrap_or("stale OHLCV stream")','reason.unwrap_or("BROKEN stream")','RECOVERY wrapper iteration 28: native event order and arguments'),
 ('recovery cooldown',source,'now - previous < 10000.','now - previous <= 10000.','RECOVERY recovery iteration 33: native event order and arguments'),
 ('dynamic market chunks',source,'new.chunks(10)','new.chunks(11)','RECOVERY markets iteration 55: native event order and arguments'),
 ('fee list fallback',source,'if truthy(&list) { list } else { empty() }','if !truthy(&list) { list } else { empty() }','RECOVERY fetchFees iteration 56: native array count'),
 ('primary markets cache',source,'!rows.borrow().is_empty()','rows.borrow().is_empty()','RECOVERY fetchMarkets iteration 64: native rejection discriminant'),
 ('market retry delay',source,'sleep.take().unwrap()(1000)','sleep.take().unwrap()(1001)','RECOVERY fetchRetry iteration 74: native event order and arguments'),
]
results=[]
def run():return subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run();assert baseline.returncode==0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout,baseline.stdout
try:
 for name,path,before,after,assertion in controls:
  text=original[path].decode();assert text.count(before)==1,(name,text.count(before))
  mutated=text.replace(before,after,1)
  path.write_text(mutated);broken=run();detected=broken.returncode==101 and 'panicked at' in broken.stdout and assertion in broken.stdout
  path.write_bytes(original[path]);restored=run();passed=restored.returncode==0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
  result=dict(family=name,path=str(path.relative_to(root)),assertion=assertion,mutation_exit=broken.returncode,named_runtime_failure=detected,restored_exit=restored.returncode,restored_pass=passed);results.append(result);print(json.dumps(result),flush=True)
  if not detected or not passed:print(broken.stdout,restored.stdout,flush=True);raise AssertionError('surviving or invalid control: '+name)
finally:
 for path,content in original.items():path.write_bytes(content)
 evidence.parent.mkdir(exist_ok=True);evidence.write_text(json.dumps(dict(gate_sha=sha,baseline_exit=baseline.returncode,command=command,controls=results,enumeration_floor=10),indent=2)+'\n')
assert all(path.read_bytes()==content for path,content in original.items())
print('Ten controls detected; exact source/test restoration verified.',flush=True)
