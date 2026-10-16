"""Sequential production controls for the four portfolio read wrappers."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/portfolio_reads.rs'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','portfolio_reads','--jobs','1']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[(name,'let Some(data) = truthy_input(data)? else','let Some(data) = truthy_input(data)?.filter(|_| false) else','portfolio_read_'+ts) for name,ts in [('balance','fetchBalance'),('positions','fetchPositions'),('open_positions','fetchOpenPositions'),('orders','fetchOrders')]]
controls += [
 ('balance','let pending = fetch_portfolio_raw();\n    async move {','async move {\n        let pending = fetch_portfolio_raw();','eager_raw_fetchBalance'),
 ('balance','let pending = fetch_portfolio_raw();','let fallback = store.balance().clone();\n    let pending = fetch_portfolio_raw();','deferred_fallback_fetchBalance'),
]
for index,(name,old,new,assertion) in enumerate(controls):
 original=source.read_bytes()
 before,body=original.decode().split('pub fn fetch_'+name+'<',1)
 body,sep,after=body.partition('\npub fn ')
 assert body.count(old)==1
 mutated=body.replace(old,new,1)
 if assertion.startswith('deferred_fallback'):mutated=mutated.replace('return Ok(store.balance().clone());','return Ok(fallback);')
 try:
  source.write_text(before+'pub fn fetch_'+name+'<'+mutated+sep+after)
  run=subprocess.run(command,env=env,capture_output=True,text=True);out=run.stdout+run.stderr
  assert run.returncode==101 and assertion in out and 'test result: FAILED' in out,(assertion,run.returncode,out)
  print(f'{index+1} {name}: FAIL exit=101 ({assertion})',flush=True)
 finally:source.write_bytes(original)
 run=subprocess.run(command,env=env,capture_output=True,text=True)
 assert run.returncode==0,(assertion,run.returncode,run.stdout+run.stderr)
 print(f'{index+1} {name}: restored PASS',flush=True)
