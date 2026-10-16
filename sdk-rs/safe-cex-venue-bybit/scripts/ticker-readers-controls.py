#!/usr/bin/env python3
"""Five new-body family controls and one reviewed join-contract witness."""
import json,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src/ticker_readers.rs'
test=root/'sdk-rs/safe-cex-venue-bybit/tests/ticker_readers.rs'
original={p:p.read_bytes() for p in [source,test]}
evidence=root/'sdk-rs/safe-cex-venue-bybit/evidence/ticker-readers-controls.json'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','ticker_readers','--jobs','2','--','--include-ignored']
controls=[
 ('without disposal',source,'if is_disposed()? {\n            Ok(TickerValue::Array','if !is_disposed()? {\n            Ok(TickerValue::Array','TICKER native without return'),
 ('parsed disposal',source,'if is_disposed()? {\n            stored_tickers()','if !is_disposed()? {\n            stored_tickers()','TICKER native settlement'),
 ('primary category',source,'let request = helper("linear");','let request = helper("inverse");','TICKER native primaryRaw eager calls'),
 ('inverse flatten order',source,'for set in found {','for set in found.into_iter().rev() {','TICKER native inverseRaw return'),
 ('helper strict status',source,'m=="OK"','m=="ok"','TICKER native settlement'),
 ('host all first rejection',test,'if let Some(error) = error {\n            Poll::Ready(Err(error))','if let Some(error) = error {\n            if pending { return Poll::Pending; }\n            Poll::Ready(Err(error))','TICKER native settlement'),
]
results=[]
def run():return subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run();assert baseline.returncode==0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout,baseline.stdout
try:
 for name,path,before,after,assertion in controls:
  text=original[path].decode();assert text.count(before)==1,(name,text.count(before))
  path.write_text(text.replace(before,after,1));broken=run();detected=broken.returncode==101 and 'panicked at' in broken.stdout and assertion in broken.stdout
  path.write_bytes(original[path]);restored=run();passed=restored.returncode==0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
  result=dict(family=name,path=str(path.relative_to(root)),assertion=assertion,mutation_exit=broken.returncode,named_runtime_failure=detected,restored_exit=restored.returncode,restored_pass=passed);results.append(result);print(json.dumps(result),flush=True)
  if not detected or not passed:print(broken.stdout,restored.stdout,flush=True);raise AssertionError('surviving or invalid control: '+name)
finally:
 for path,content in original.items():path.write_bytes(content)
 evidence.parent.mkdir(exist_ok=True);evidence.write_text(json.dumps(dict(gate_sha=sha,baseline_exit=baseline.returncode,command=command,controls=results,enumeration_floor=6),indent=2)+'\n')
assert all(path.read_bytes()==content for path,content in original.items())
print('Six controls detected; exact source/test restoration verified.',flush=True)
