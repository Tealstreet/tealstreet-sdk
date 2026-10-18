"""One compiling discrimination control per new family, restored between runs."""
from pathlib import Path
import os, subprocess, json
command=['/opt/homebrew/opt/rustup/bin/cargo','test','-p','safe-cex-venue-woo','--test','v3_request_ticker','--jobs','1','--offline','--locked']
crate=Path(__file__).resolve().parents[1]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH=str(crate.parents[1]/'node_modules/typescript'))
controls=[
 ('ticker','v3_ticker.rs','bid: require_positive_finite_price(&bbo.bid_price, "WOO V3 ticker bid")?','bid: require_positive_finite_price(&bbo.ask_price, "WOO V3 ticker bid")?','COMPOSE native bid'),
 ('quotes','v3_ws_quotes.rs','#[serde(rename = "askPrice")]','#[serde(rename = "bidPrice")]','BBO native ask price'),
]
record={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'command':command,'iterations':[]}
try:
 for name,file,before,after,assertion in controls:
  path=crate/'src'/file;original=path.read_text()
  assert original.count(before)==1,(name,'admission')
  mutated=original.replace(before,after,1)
  if name=='quotes':
   # Symmetric Serde renames can survive roundtrips; native fields must catch it.
   mutated=mutated.replace('#[serde(rename = "bidPrice")]\n    bid_price:', '#[serde(rename = "askPrice")]\n    bid_price:',1)
  try:
   path.write_text(mutated)
   run=subprocess.run(command,cwd=crate.parent,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
   Path('/tmp/woo-v3-control-'+name+'.log').write_text(run.stdout)
   caught=run.returncode==101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
   record['iterations'].append({'family':name,'exit':run.returncode,'assertion':assertion,'caught':caught})
   Path('/tmp/woo-v3-controls.json').write_text(json.dumps(record,indent=2)+'\n')
   assert caught,(name,run.stdout)
   print(name,101,assertion,flush=True)
  finally:path.write_text(original)
 run=subprocess.run(command,cwd=crate.parent,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-v3-control-restored.log').write_text(run.stdout)
 record['restored_exit']=run.returncode
 assert run.returncode==0,run.stdout
 print('restored target passed',flush=True)
finally:Path('/tmp/woo-v3-controls.json').write_text(json.dumps(record,indent=2)+'\n')
