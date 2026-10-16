#!/usr/bin/env python3
"""One compiling native fault per new helper family; serial Cargo and exact restoration."""
from pathlib import Path
import os,subprocess
root=Path(__file__).resolve().parents[3]
folder=root/'sdk-rs/safe-cex-venue-bybit/src'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','exchange_helpers','--jobs','2','--','--include-ignored']
print('candidate SHA',subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),flush=True)
controls=[
 ('string-guards','exchange_helpers.rs','value.and_then(Value::as_str).filter(|s| !s.is_empty())','value.and_then(Value::as_str).filter(|s| s.is_empty())','HELPER native absent stringOrUndefined'),
 ('numeric-guards','exchange_helpers.rs','Ok(number.is_finite().then_some(number))','Ok(Some(if number.is_finite() { number } else { 0. }))','HELPER native absent parseBybitFiniteNumber'),
 ('mode-predicate','exchange_helpers.rs','Some("Full" | "Partial")','Some("FullX" | "Partial")','HELPER native mode'),
 ('message-predicates','exchange_helpers.rs','.map(|s| s.to_lowercase())','.map(|s| s.to_lowercase().replace("order not exists", ""))','HELPER native message predicate'),
 ('coin-fold','exchange_helpers.rs','coin_sum += value;','coin_sum -= value;','HELPER native sumBybitCoinFields'),
 ('wallet-balance','exchange_helpers.rs','math_max(wallet - used, 0.)','math_max(used - wallet, 0.)','HELPER native deriveBybitAvailableBalanceFromCoins'),
 ('first-finite','exchange_helpers.rs','.find(|v| v.is_finite())','.find(|v| !v.is_finite())','HELPER native firstFiniteNumber'),
 ('inverse-trigger','order_format.rs','Some(Trigger::MarkPrice) => ticker.mark,','Some(Trigger::MarkPrice) => ticker.index,','HELPER native inverse trigger direction'),
]
for family,filename,before,after,assertion in controls:
 path=folder/filename;original=path.read_text();assert original.count(before)==1,(family,'selector drift')
 try:
  path.write_text(original.replace(before,after));r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  print(family,'mutated exit',r.returncode,flush=True);print(r.stdout,flush=True)
  assert r.returncode==101 and assertion in r.stdout and 'panicked at' in r.stdout,(family,'SURVIVOR or non-runtime failure')
 finally:path.write_text(original)
 r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 print(family,'restored exit',r.returncode,flush=True);print(r.stdout,flush=True)
 assert r.returncode==0 and '1 passed; 0 failed; 0 ignored' in r.stdout,(family,'restore did not pass')
