"""Sequential, scoped mutation controls; every corpus invokes Node once."""
from pathlib import Path
import json, os, subprocess
root=Path(__file__).resolve().parents[3]
os.chdir(root)
prod=Path('sdk-rs/safe-cex-venue-paradex/src/order_reads.rs')
test=Path('sdk-rs/safe-cex-venue-paradex/tests/order_reads.rs')
oracle=Path('sdk-rs/safe-cex-venue-paradex/tests/order_reads_oracle.cjs')
original={p:p.read_text() for p in [prod,test,oracle]}
s=original[prod];t=original[test];o=original[oracle]
needle='open_orders.into_iter().map(&mut map_order).collect()'
empty=s.replace(needle,'let mut output = open_orders.into_iter().map(&mut map_order).collect::<Result<Vec<T>, E>>()?; output.clear(); Ok(output)')
subset=empty.replace('output.clear();','output.pop();')
extra=s.replace(needle,'let extra = open_orders.first().copied(); let mut output = open_orders.into_iter().map(&mut map_order).collect::<Result<Vec<T>, E>>()?; if let Some(row) = extra { output.push(map_order(row)?); } Ok(output)')
guard='assert_eq!(cases.len(), 36, "orders_source_cases");'
assert guard in t
controls=[
 ('empty',{prod:empty},'orders_native_cardinality'),
 ('subset',{prod:subset},'orders_native_cardinality'),
 ('extra',{prod:extra},'orders_native_cardinality'),
 ('empty-source-count-bypass',{prod:empty,test:t.replace(guard,'')},'orders_native_cardinality'),
 ('pending-status',{prod:s.replace('"NEW" | "OPEN" | "PENDING" | "UNTRIGGERED"','"NEW" | "OPEN" | "UNTRIGGERED"')},'orders_native_cardinality'),
 ('discard-init',{prod:s.replace('init.await?;','drop(init);')},'orders_before_init'),
 ('wrong-endpoint',{prod:s.replace('*k == "ORDERS"','*k == "ACCOUNT"')},'orders_before_init'),
 ('source-reversal',{oracle:o.replace('for(const c of defs)','for(const c of defs.reverse())')},'orders_live_input_content'),
 ('source-same-length-replacement',{oracle:o.replace('const cases=[];for(const c of defs)','const cases=[];defs[1]=defs[0];for(const c of defs)')},'orders_live_input_content'),
]
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-paradex','--test','order_reads','--jobs','1']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('.aimux/paradex');logs.mkdir(parents=True,exist_ok=True);report=[]
for name,edits,assertion in controls:
 try:
  for p,text in edits.items():
   assert text!=original[p],name
   p.write_text(text)
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr;(logs/f'order-reads-{name}.log').write_text(out)
 finally:
  for p,text in original.items():p.write_text(text)
 assert r.returncode==101 and 'test result: FAILED' in out and assertion in out,(name,out)
 r2=subprocess.run(cmd,env=env,capture_output=True,text=True);(logs/f'order-reads-{name}-restored.log').write_text(r2.stdout+r2.stderr)
 assert r2.returncode==0,(name,r2.stdout,r2.stderr)
 report.append(dict(control=name,exit=r.returncode,assertion=assertion,restored_exit=r2.returncode));print(json.dumps(report[-1]),flush=True)
 (logs/'order-reads-controls.json').write_text(json.dumps(report,indent=2)+'\n')
