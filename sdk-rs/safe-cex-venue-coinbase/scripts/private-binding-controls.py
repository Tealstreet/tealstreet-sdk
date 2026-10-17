#!/usr/bin/env python3
"""Prove the actual private frame bridge and emitted fill event are observed."""
import hashlib, os, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
source=root/'safe-cex-venue-coinbase/src/binding/sockets.rs'
original=source.read_bytes()
text=original.decode()
bridge='.on_message(&arg(&args, 0).get("data")?.to_display())'
assert text.count(bridge)==2
fill='self.emit(\n            "fill",'
assert text.count(fill)==1
mutants=[
    ('private-frame-dispatch','real_private_order_snapshot_populates_store','real private snapshot must reach Store',text.replace(bridge,'.on_message("{}")',1)),
    ('private-fill-emission','real_private_filled_update_closes_order_and_emits_fill','real filled private update must emit exact fill payload',text.replace(fill,'self.emit(\n            "fixture-suppressed-fill",')),
]
def run(target=None):
    args=['cargo','test','--locked','-p','safe-cex-venue-coinbase','--jobs=1','--test','private_binding_frames']
    if target:args += [target,'--','--exact','--nocapture']
    return subprocess.run(args,cwd=root,env={**os.environ,'RUSTC_BOOTSTRAP':'1'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
baseline=run();assert baseline.returncode==0,baseline.stdout
print('BASELINE actual private frames4tests / bothvariants PASS',flush=True)
for name,target,label,mutant in mutants:
    assert source.read_bytes()==original
    try:
        source.write_text(mutant)
        result=run(target)
        assert result.returncode==101 and 'test result: FAILED' in result.stdout and label in result.stdout,result.stdout
        print(result.stdout,flush=True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes()==original
    restored=run(target);assert restored.returncode==0,restored.stdout
    print('CONTROL '+name+': compiled named RED / exact-restored PASS',flush=True)
final=run();assert final.returncode==0,final.stdout
assert source.read_bytes()==original
print('RESTORED actual private frames4tests PASS; SHA256 '+hashlib.sha256(original).hexdigest(),flush=True)
