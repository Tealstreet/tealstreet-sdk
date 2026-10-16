"""Mutate one production/corpus seam at a time; one scoped Cargo and Node corpus."""
from pathlib import Path
import os, subprocess, json
os.chdir(Path(__file__).resolve().parents[3])
p=Path('sdk-rs/safe-cex-venue-paradex/src/order_maps.rs');o=Path('sdk-rs/safe-cex-venue-paradex/tests/order_maps_oracle.cjs');original={p:p.read_text(),o:o.read_text()};s=original[p];oracle=original[o]
controls=[
 ('conditional-guard',p,s.replace('!reduce_only.unwrap_or(true)','reduce_only.unwrap_or(true)'),'maps_native_text'),
 ('filled-status',p,s.replace('("FILLED", OrderStatus::Closed)','("FILLED", OrderStatus::Canceled)'),'maps_native_text'),
 ('type-inherited',p,s.replace('TableHit::Inherited(member) => TableHit::Inherited(member),','TableHit::Inherited(_) => TableHit::Mapped(OrderType::Limit),',1),'maps_native_kind'),
 ('status-inherited',p,s.replace('TableHit::Inherited(member) => TableHit::Inherited(member),\n        TableHit::Missing => TableHit::Mapped(OrderStatus::Open),','TableHit::Inherited(_) => TableHit::Mapped(OrderStatus::Open),\n        TableHit::Missing => TableHit::Mapped(OrderStatus::Open),'),'maps_native_kind'),
 ('same-length-replacement',o,oracle.replace('const cases=keys.map','keys[0]=keys[1];const cases=keys.map'),'maps_live_input_content'),
 ('reversal',o,oracle.replace('const cases=keys.map','keys.reverse();const cases=keys.map'),'maps_live_input_content'),
]
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-paradex','--test','order_maps','--jobs','1'];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('.aimux/paradex');report=[]
for name,path,code,assertion in controls:
 assert code!=original[path],name
 try:
  path.write_text(code);r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr;(logs/f'order-maps-{name}.log').write_text(out)
 finally:path.write_text(original[path])
 assert r.returncode==101 and 'test result: FAILED' in out and assertion in out,(name,out)
 r2=subprocess.run(cmd,env=env,capture_output=True,text=True);(logs/f'order-maps-{name}-restored.log').write_text(r2.stdout+r2.stderr);assert r2.returncode==0,(name,r2.stdout,r2.stderr)
 report.append(dict(control=name,exit=r.returncode,assertion=assertion,restored_exit=r2.returncode));print(json.dumps(report[-1]),flush=True);(logs/'order-maps-controls.json').write_text(json.dumps(report,indent=2)+'\n')
