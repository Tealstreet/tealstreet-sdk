#!/usr/bin/env python3
"""One control per new recovery/read method, run sequentially."""
import json,os,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src/core_snapshot_readers.rs'
test=root/'sdk-rs/safe-cex-venue-bybit/tests/core_snapshot_readers.rs'
original={p:p.read_bytes() for p in [source,test]}
evidence=root/'sdk-rs/safe-cex-venue-bybit/evidence/core-snapshot-readers-controls.json'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','core_snapshot_readers','--jobs','2','--','--include-ignored']
controls=[
 ('parallel raw balance slot',source,'let raw = h.get(&values, "2")?;','let raw = h.get(&values, "1")?;','SNAPSHOT all iteration 0: dependency order and arguments'),
 ('ordered optional orders',source,'let orders = if fetch_orders {','let orders = if !fetch_orders {','SNAPSHOT snapshot iteration 8: native store0.orders: native array count'),
 ('wrapper rejection',source,'let p = call!(h, owner, "fetchAndSetPrivateSnapshotHelper", vec![]);\n        h.wait(p).await?;','let p = call!(h, owner, "fetchAndSetPrivateSnapshotHelper", vec![]);\n        let _ = h.wait(p).await;','SNAPSHOT wrapper iteration 17: native rejection discriminant'),
 ('tick backoff cap',source,'h.number(300000.)','h.number(300001.)','SNAPSHOT tick iteration 25: native exchange state: native f64'),
 ('sync original allocation',source,'h.equal(&balance, &h.get(&h.get(owner, "store")?, "balance")?)','!h.equal(&balance, &h.get(&h.get(owner, "store")?, "balance")?)','SNAPSHOT sync iteration 30: native store0.balance: native object count'),
 ('unified raw balance status',source,'h.set(&owner, "unifiedMarginStatus", h.number(6.))?;','h.set(&owner, "unifiedMarginStatus", h.number(7.))?;','SNAPSHOT raw iteration 40: native exchange state: native f64'),
 ('orders preserve call',source,'let preserve = h.select(owner, "preserveRecentlyPlacedOrders")?;\n        let flat = call!(h, orders, "flat", vec![]);\n        h.apply(preserve, vec![flat])','let _preserve = h.select(owner, "preserveRecentlyPlacedOrders")?;\n        let flat = call!(h, orders, "flat", vec![]);\n        Ok(flat)','SNAPSHOT orders iteration 50: native rejection discriminant'),
 ('order paging limit',source,'("limit", h.number(50.))','("limit", h.number(51.))','SNAPSHOT ordersHelper iteration 52: eager effects'),
 ('empty symbol delegation',source,'h.numeric(&h.optional(&symbols, "length")?)? > 0.','h.numeric(&h.optional(&symbols, "length")?)? >= 0.','SNAPSHOT positions iteration 64: native array count'),
 ('position paging limit',source,'("limit", h.number(200.))','("limit", h.number(201.))','SNAPSHOT positionsHelper iteration 68: eager effects'),
]
start=int(sys.argv[1]) if len(sys.argv)>1 else 0
assert 0<=start<10 and len(controls)==10
evidence=evidence.with_name("core-snapshot-readers-controls-from"+str(start)+".json")
results=[]
def run():return subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run();assert baseline.returncode==0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout,baseline.stdout
try:
 for name,path,before,after,assertion in controls[start:]:
  text=original[path].decode();assert text.count(before)==1,(name,text.count(before))
  mutated=text.replace(before,after,1)
  path.write_text(mutated);broken=run();detected=broken.returncode==101 and 'panicked at' in broken.stdout and assertion in broken.stdout
  path.write_bytes(original[path]);restored=run();passed=restored.returncode==0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
  result=dict(family=name,path=str(path.relative_to(root)),assertion=assertion,mutation_exit=broken.returncode,named_runtime_failure=detected,restored_exit=restored.returncode,restored_pass=passed);results.append(result);print(json.dumps(result),flush=True)
  if not detected or not passed:print(broken.stdout,restored.stdout,flush=True);raise AssertionError('surviving or invalid control: '+name)
finally:
 for path,content in original.items():path.write_bytes(content)
 evidence.parent.mkdir(exist_ok=True);evidence.write_text(json.dumps(dict(gate_sha=sha,baseline_exit=baseline.returncode,command=command,controls=results,planned_floor=10-start,enumeration_floor=len(results),start_iteration=start,last_iteration=start+len(results)-1),indent=2)+'\n')
assert all(path.read_bytes()==content for path,content in original.items())
assert len(results)==10-start
print(str(len(results))+' controls detected; stopped at iteration 9; exact source/test restoration verified.',flush=True)
