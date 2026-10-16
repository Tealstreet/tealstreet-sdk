#!/usr/bin/env python3
"""Ten row families, sequential runtime mutation / exact restoration."""
import json,os,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src/ledger_history.rs'
test=root/'sdk-rs/safe-cex-venue-bybit/tests/ledger_history.rs'
original={p:p.read_bytes() for p in [source,test]}
evidence=root/'sdk-rs/safe-cex-venue-bybit/evidence/ledger-history-controls.json'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','ledger_history','--jobs','2','--','--include-ignored']
controls=[
 ('income context type unions',source,'Self::Contract => "CONTRACT"','Self::Contract => "OTHER"','LEDGER type account union'),
 ('raw closed page cap',source,'if !h.truthy(&cursor) || page >= 20 {\n                        break','if !h.truthy(&cursor) || page >= 19 {\n                        break','LEDGER raw iteration 8: native array count'),
 ('hedge leg join',source,'"fetchHistoricalOrders",\n                vec![h.object','"fetchClosedPositionRows",\n                vec![h.object','LEDGER positions iteration 12.leg: native undefined'),
 ('mapped closed account',source,'h.get(&opts, "accountName")?','h.get(&opts, "account")?','LEDGER closed iteration 22.account: native UTF16'),
 ('historical first-id dedupe',source,'call!(h,seen,"add",vec![h.get(&order,"id")?]);','let _=seen;','LEDGER orders iteration 39: native array count'),
 ('fresh primary income context',source,'category: IncomeCategory::Linear','category: IncomeCategory::Inverse','LEDGER context iteration 49.category: native UTF16'),
 ('income wrapper return adoption',source,'h.apply(f, vec![ctx, opts])','let _=(f,ctx,opts); Ok(h.array(vec![]))','LEDGER wrapperIncome iteration 53: native array count'),
 ('range wrapper return adoption',source,'h.apply(f, vec![ctx, start, end, income_type])','let _=(f,ctx,start,end,income_type); Ok(h.array(vec![]))','LEDGER wrapperRange iteration 57: native array count'),
 ('income NaN cap condition',source,'!(h.numeric(&h.get(&records, "length")?)?\n                                < h.numeric(&record_cap)?)','(h.numeric(&h.get(&records, "length")?)? >= h.numeric(&record_cap)?)','LEDGER income iteration 75: native array count'),
 ('inclusive income range advance',source,'vec![current_end, h.number(1.)]','vec![current_end, h.number(604800000.)]','LEDGER wrapperRange iteration 58: native array count'),
]

start=int(sys.argv[1]) if len(sys.argv)>1 else 0
assert 0<=start<10 and len(controls)==10
evidence=evidence.with_name("ledger-history-controls-from"+str(start)+".json")
results=[]
def run():return subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
for name,path,before,after,assertion in controls:assert original[path].decode().count(before)==1,(name,original[path].decode().count(before))
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
