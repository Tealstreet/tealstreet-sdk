"""Sequential production controls; one actual-source Node corpus per test process."""
from pathlib import Path
import os,subprocess,argparse
parser=argparse.ArgumentParser();parser.add_argument("--start-at",type=int,default=0);args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/market_reads.rs';test=crate/'tests/combined.rs'
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','combined','--jobs','1']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('last instead of first','.position(|i| i.symbol == detail.symbol)','.rposition(|i| i.symbol == detail.symbol)','COMBINED-NATIVE fields',False),
 ('empty output','js_from_value(Value::Array(output))','output.clear(); js_from_value(Value::Array(output))','COMBINED-NATIVE independent output count',False),
 ('subset output','js_from_value(Value::Array(output))','output.truncate(1); js_from_value(Value::Array(output))','COMBINED-NATIVE independent output count',False),
 ('extra output','js_from_value(Value::Array(output))','output.push(serde_json::json!({})); js_from_value(Value::Array(output))','COMBINED-NATIVE independent output count',False),
 ('empty bypass guards','js_from_value(Value::Array(output))','output.clear(); js_from_value(Value::Array(output))','COMBINED-NATIVE fields',True),
 ('detail field swap','"instrumentDetails": right_raw["instrumentDetails"][detail_index].clone()','"instrumentDetails": ({ let mut row=right_raw["instrumentDetails"][detail_index].clone(); let min=row["minOrderSize"].clone(); row["minOrderSize"]=row["minOrderSizeIncrement"].clone(); row["minOrderSizeIncrement"]=min; row })','COMBINED-NATIVE fields',False),
 ('serialize pending polls','if right_value.is_none() {','if left_value.is_some() && right_value.is_none() {','COMBINED-CONCURRENT right polled while left pending',False),
]
for name,old,new,assertion,bypass in controls[args.start_at:]:
 original=source.read_bytes();test_original=test.read_bytes();s=original.decode();assert s.count(old)==1,(name,s.count(old))
 try:
  source.write_text(s.replace(old,new))
  if bypass:
   t=test_original.decode();start=t.index('            assert_eq!(\n                output.as_array()');end=t.index('            assert_eq!(\n                observe(&output)',start)
   t=t[:start]+t[end:];test.write_text(t)
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: compiled; exit101; {assertion}',flush=True)
 finally:source.write_bytes(original);test.write_bytes(test_original)
 r=subprocess.run(cmd,env=env,capture_output=True,text=True)
 assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)

# Remove a source duplicate without changing the expected output count.
oracle=crate/'tests/combined_oracle.cjs'; original=oracle.read_bytes()
try:
 s=original.decode();old="instrument('A',11),instrument('B',23),instrument('A',37)";assert s.count(old)==1
 oracle.write_text(s.replace(old,"instrument('A',11),instrument('B',23)"))
 r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
 assert r.returncode==101 and 'COMBINED-INPUT instrument count' in out and 'test result: FAILED' in out,(r.returncode,out)
 print('source duplicate removal: exit101; COMBINED-INPUT instrument count',flush=True)
finally:oracle.write_bytes(original)
r=subprocess.run(cmd,env=env,capture_output=True,text=True)
assert r.returncode==0,(r.returncode,r.stdout+r.stderr)
print('source duplicate removal: restored PASS',flush=True)
