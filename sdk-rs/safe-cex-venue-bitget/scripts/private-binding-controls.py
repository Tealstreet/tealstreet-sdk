#!/usr/bin/env python3
"""Require real private-topic Store/fill failures after narrow production reversions."""
import hashlib,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
host=root/'safe-cex-venue-bitget/src/binding/hosts/private.rs'
topics=root/'safe-cex-venue-bitget/src/private_topics.rs'
originals={p:p.read_bytes() for p in [host,topics]}
text=originals[host].decode()
mutants=[]
for tag,member,target,label in [
 ('order-dispatch','handle_order_topic','real_private_order_frame_populates_store_without_fill','real orders frame must reach Store'),
 ('position-dispatch','handle_positions_topic','real_private_position_snapshot_updates_native_position','real position frame must preserve fresh WS contracts'),
 ('account-dispatch','handle_account_topic','real_private_account_frame_updates_native_balance','real account frame must reach Store balance'),
]:
 old='Rc::new(|rt, a| unit(private_topics::'+member+'(rt, arg(&a, 0))))'
 assert text.count(old)==1
 mutants.append((tag,host,target,label,text.replace(old,'Rc::new(|rt, a| { let _ = (rt, a); Ok(Js::Undefined) })')))
fill='vec![s(h, "fill"), fill]'
assert originals[topics].decode().count(fill)==2
mutants.append(('fill-emission',topics,'real_private_fill_closes_known_order_and_emits_exact_payload','real fill must emit exact native contents',originals[topics].decode().replace(fill,'vec![s(h, "fixture-suppressed-fill"), fill]',1)))
def run(target=None):
 args=['cargo','test','--locked','-p','safe-cex-venue-bitget','--jobs=1','--test','private_binding_frames']
 if target:args += [target,'--','--exact','--nocapture']
 return subprocess.run(args,cwd=root,env={**os.environ,'RUSTC_BOOTSTRAP':'1'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
baseline=run();assert baseline.returncode==0,baseline.stdout
print('BASELINE actual orders/fills/positions/account4PASS',flush=True)
for name,source,target,label,mutant in mutants:
 assert all(p.read_bytes()==b for p,b in originals.items())
 try:
  source.write_text(mutant)
  result=run(target)
  assert result.returncode==101 and 'test result: FAILED' in result.stdout and label in result.stdout,result.stdout
  print(result.stdout,flush=True)
 finally:
  source.write_bytes(originals[source])
  assert all(p.read_bytes()==b for p,b in originals.items())
 restored=run(target);assert restored.returncode==0,restored.stdout
 print('CONTROL '+name+': compiled named RED / exact-restored PASS',flush=True)
final=run();assert final.returncode==0,final.stdout
assert all(p.read_bytes()==b for p,b in originals.items())
print('RESTORED actual private frames4PASS; '+str({str(p.relative_to(root)):hashlib.sha256(b).hexdigest() for p,b in originals.items()}),flush=True)
