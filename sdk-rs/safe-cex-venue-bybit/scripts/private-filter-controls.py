#!/usr/bin/env python3
"""Compile independent real-frame faults, exact restore, and replay GREEN."""
from pathlib import Path
import hashlib,json,os,subprocess
crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
source=crate/'src/ws_private/messages.rs'
original=source.read_bytes()
text=original.decode()
env=dict(os.environ,CARGO_INCREMENTAL='0',SAFE_CEX_TYPESCRIPT_PATH=os.environ.get('SAFE_CEX_TYPESCRIPT_PATH','/Users/sam/cs/tealstreet-sdk/node_modules/typescript'))
command=['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-bybit','--all-features','--lib','private_filter_reads_live_indices_through_real_frame_binding','--','--nocapture']
controls=[
 ('snapshot-indices','let accepted = rows.call_method(', 'let snapshot = Js::array(rows.array_values()?.unwrap_or_default());\n        let accepted = snapshot.call_method('),
 ('ignore-selector','this.call_method("tradesCategoryOf", vec![symbol])','let _ = this.call_method("tradesCategoryOf", vec![symbol])?;\n                Ok(Js::Bool(true))'),
]
records=[]
def run(label):
    log=Path('/tmp/bybit-private-filter-'+label+'.log')
    with log.open('w') as stream:
        result=subprocess.run(command,cwd=sdk,env=env,stdout=stream,stderr=subprocess.STDOUT)
    return result.returncode,log,log.read_text()
try:
    for name,old,new in controls:
        assert text.count(old)==1,(name,text.count(old))
        source.write_text(text.replace(old,new))
        code,red,output=run(name+'-red')
        assert code!=0 and 'error[E' not in output and 'private_filter_reads_live_indices_through_real_frame_binding ... FAILED' in output,(name,red)
        source.write_bytes(original)
        assert source.read_bytes()==original
        code,green,output=run(name+'-restore-green')
        assert code==0 and '1 passed; 0 failed' in output,(name,green)
        records.append(dict(control=name,compiled_runtime_red=True,red_log=str(red),red_sha256=hashlib.sha256(red.read_bytes()).hexdigest(),exact_restore=True,green=True,green_log=str(green),green_sha256=hashlib.sha256(green.read_bytes()).hexdigest()))
        print(json.dumps(records[-1]),flush=True)
finally:
    source.write_bytes(original)
assert source.read_bytes()==original
out=crate/'evidence/private-filter-controls.json'
out.write_text(json.dumps(dict(source_sha256=hashlib.sha256(original).hexdigest(),executed=len(records),controls=records),indent=2)+'\n')
