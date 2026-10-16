#!/usr/bin/env python3
"""Six new primary order lifecycle family controls."""
import json,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src/primary_order_lifecycle.rs'
test=root/'sdk-rs/safe-cex-venue-bybit/tests/primary_order_lifecycle.rs'
original={p:p.read_bytes() for p in [source,test]}
evidence=root/'sdk-rs/safe-cex-venue-bybit/evidence/primary-order-lifecycle-controls.json'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','primary_order_lifecycle','--jobs','2','--','--include-ignored']
controls=[
 ('demo fallback',source,'if is_demo()? {','if !is_demo()? {','PRIMARY native fallback'),
 ('timer lifecycle',source,'[1000., 3000., 8250.]','[1000., 3001., 8250.]','PRIMARY native number timer delay'),
 ('filled target',source,'order.remaining + filled','order.remaining - filled','PRIMARY native number target'),
 ('recent record',source,'order.borrow().r#type == OrderType::Market','order.borrow().r#type == OrderType::Limit','PRIMARY native cases case 1 step 0 record: event count'),
 ('retired symbol TTL',source,'now - entry.retired_at > 60000.','now - entry.retired_at >= 60000.','PRIMARY native cases case 7 step 2 resolve: absent symbol'),
 ('captured mutable entry',source,'let store = store_orders()?;\n        let order = store','let captured_order = entry.borrow().order.clone();\n        let store = store_orders()?;\n        let order = store','PRIMARY native cases case 10 step 2 preserve: original preserved order identity'),
]
results=[]
def run():return subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run();assert baseline.returncode==0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout,baseline.stdout
try:
 for name,path,before,after,assertion in controls:
  text=original[path].decode();assert text.count(before)==1,(name,text.count(before))
  mutated=text.replace(before,after,1)
  if name=='captured mutable entry':
   assert mutated.count('.unwrap_or_else(|| entry.borrow().order.clone())')==1
   mutated=mutated.replace('.unwrap_or_else(|| entry.borrow().order.clone())','.unwrap_or_else(|| captured_order.clone())',1)
  path.write_text(mutated);broken=run();detected=broken.returncode==101 and 'panicked at' in broken.stdout and assertion in broken.stdout
  path.write_bytes(original[path]);restored=run();passed=restored.returncode==0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
  result=dict(family=name,path=str(path.relative_to(root)),assertion=assertion,mutation_exit=broken.returncode,named_runtime_failure=detected,restored_exit=restored.returncode,restored_pass=passed);results.append(result);print(json.dumps(result),flush=True)
  if not detected or not passed:print(broken.stdout,restored.stdout,flush=True);raise AssertionError('surviving or invalid control: '+name)
finally:
 for path,content in original.items():path.write_bytes(content)
 evidence.parent.mkdir(exist_ok=True);evidence.write_text(json.dumps(dict(gate_sha=sha,baseline_exit=baseline.returncode,command=command,controls=results,enumeration_floor=6),indent=2)+'\n')
assert all(path.read_bytes()==content for path,content in original.items())
print('Six controls detected; exact source/test restoration verified.',flush=True)
