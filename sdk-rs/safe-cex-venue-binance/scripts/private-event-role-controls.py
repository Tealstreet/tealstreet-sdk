#!/usr/bin/env python3
"""Disconnect each actual private-event role; require native failures and restore."""
from pathlib import Path
import re
import subprocess
import sys
if not __debug__:
    raise SystemExit('Controls require Python assertions; do not use -O')
workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-binance/src/private_event_value.rs'
original=source.read_bytes()
logs=workspace/'target/private-event-role-controls';logs.mkdir(parents=True,exist_ok=True)
orders='production_native_private_orders_frames_update_the_real_store'
positions='production_native_private_position_frames_update_the_real_store'
stock='event_stock_reads_keep_current_realm_referents_and_later_properties'
controls=[
 ('wire','private_classes',orders,b'\n let _=self; Value::Null\n'),
 ('array_for_each','private_classes',positions,b'\n let _=(value,expression,visit); Ok(())\n'),
 ('record','private_classes',orders,b'\n let _=fields; EventValue::object(vec![])\n'),
 ('stock','accepted_receivers',stock,b'\n let _=key; EventValue::Undefined\n'),
 ('string','private_classes',orders,b'\n let _=v; EventValue::str("incorrect-literal")\n'),
 ('table','private_classes',orders,b'\n let _=(entries,key,wrap); Ok(EventValue::Undefined)\n'),
]
assert len(sys.argv)<=2
chosen=None if len(sys.argv)==1 else sys.argv[1].removeprefix('--only=')
assert chosen is None or (sys.argv[1].startswith('--only=') and chosen in [r[0] for r in controls])
def run(target,test,label):
    r=subprocess.run(['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-binance','--features','native','--test',target,test,'--','--exact'],cwd=workspace,capture_output=True,text=True,timeout=1200)
    output=r.stdout+r.stderr;(logs/(label+'.log')).write_text(output)
    return r.returncode,output
try:
 for name,target,test,replacement in controls:
    if chosen is not None and name != chosen:continue
    assert source.read_bytes()==original
    markers=list(re.finditer(rb'\bfn '+name.encode()+rb'[<(]',original));assert len(markers)==1,name
    start=original.index(b'{',markers[0].end());end=start+1;depth=1
    while depth:
        depth+=(original[end]==123)-(original[end]==125);end+=1
    # Retain otherwise sole imports in the table fault. This changes
    # the result/effect, never relaxes compilation or an identity/alias guard.
    if name=='table':
        body=original[start+1:end-1]
        before=b'TableHit::Mapped(v) => EventValue::enum_value(wrap(*v))'
        assert body.count(before)==1
        replacement=body.replace(before,b'TableHit::Mapped(v) => { let _=wrap(*v); EventValue::Undefined }')
    source.write_bytes(original[:start+1]+replacement+original[end-1:])
    code,output=run(target,test,name+'-red')
    assert code==101 and f'test {test} ... FAILED' in output and 'could not compile' not in output,output[-8000:]
    print(name+': compiled named production-binding RED',flush=True)
    source.write_bytes(original);assert source.read_bytes()==original
    code,output=run(target,test,name+'-restored')
    assert code==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-8000:]
    print(name+': byte-exact restored production-binding GREEN',flush=True)
finally:
 source.write_bytes(original)
