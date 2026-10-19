#!/usr/bin/env python3
"""Compile a real-auth numeric capture fault, exact restore, and replay GREEN."""
from pathlib import Path
import hashlib,json,os,subprocess
crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
source=crate/'src/ws_private/local_auth.rs'
original=source.read_bytes()
text=original.decode()
env=dict(os.environ,CARGO_INCREMENTAL='0')
command=['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-bybit','--all-features','--lib','inverse_auth_production_clock_keeps_finite_expiry_at_real_signer','--','--nocapture']
controls=[
 ('null-expiry','if expires.is_finite() {','if expires.is_finite() && expires.is_nan() {'),
]
records=[]
def run(label):
    log=Path('/tmp/bybit-inverse-auth-clock-'+label+'.log')
    with log.open('w') as stream:
        result=subprocess.run(command,cwd=sdk,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=600)
    return result.returncode,log,log.read_text()
try:
    for name,old,new in controls:
        assert text.count(old)==1,(name,text.count(old))
        source.write_text(text.replace(old,new))
        code,red,output=run(name+'-red')
        assert code!=0 and 'error[E' not in output and 'inverse_auth_production_clock_keeps_finite_expiry_at_real_signer ... FAILED' in output,(name,red)
        source.write_bytes(original)
        assert source.read_bytes()==original
        code,green,output=run(name+'-restore-green')
        assert code==0 and '1 passed; 0 failed' in output,(name,green)
        records.append(dict(control=name,compiled_runtime_red=True,red_log=str(red),red_sha256=hashlib.sha256(red.read_bytes()).hexdigest(),exact_restore=True,green=True,green_log=str(green),green_sha256=hashlib.sha256(green.read_bytes()).hexdigest()))
        print(json.dumps(records[-1]),flush=True)
finally:
    source.write_bytes(original)
assert source.read_bytes()==original
out=crate/'evidence/inverse-auth-clock-controls.json'
out.write_text(json.dumps(dict(source_sha256=hashlib.sha256(original).hexdigest(),executed=len(records),controls=records),indent=2)+'\n')
