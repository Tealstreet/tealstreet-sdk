from pathlib import Path
import os,subprocess,json,re
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-phemex/src/rest_data.rs'
out=root/'sdk-rs/target/phemex-rest-data-evidence';out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('markets','fetch_markets','let min = h.num(1.0);','let min = h.num(2.0);','RESTDATA native complete observation'),
 ('position','map_position','size.abs() * entry','size.abs() * entry * 2.0','RESTDATA native complete observation'),
 ('positions','fetch_positions','h.flatten(positions)','h.array(positions)','RESTDATA native complete observation'),
 ('orders','fetch_orders','h.store("orders")','h.store("positions")','RESTDATA native orders identity'),
 ('order_update','map_order_update','h.num(amount)','h.num(amount + 1.0)','RESTDATA native complete observation'),
 ('balance','fetch_positions_and_balance','h.num(subtract(total, used))','h.num(subtract(total, used) + 1.0)','RESTDATA native complete observation'),
 ('fills','fetch_historical_fills','math_max(h.coerce(limit)?, 1.0)','math_max(h.coerce(limit)?, 2.0)','RESTDATA native effect get'),
 ('candles','fetch_ohlcv','math_min(limit, 1000.0)','math_min(limit, 999.0)','RESTDATA native effect get'),
]
original=source.read_text();mutated=original
for family,function,before,after,assertion in controls:
 markers=['pub fn '+function+'<','pub async fn '+function+'<'];start=next(mutated.index(m) for m in markers if m in mutated)
 ends=[mutated.find('\npub ',start+1),mutated.find('\nfn ',start+1)];end=min((e for e in ends if e!=-1),default=len(mutated));block=mutated[start:end]
 assert block.count(before)==1,(family,before,block.count(before))
 mutated=mutated[:start]+block.replace(before,after)+mutated[end:]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','rest_data','--','--nocapture','--test-threads=1']
try:
 source.write_text(mutated)
 failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/'mutated.log').write_text(failed.stdout)
 assert failed.returncode==101 and 'Finished `test`' in failed.stdout,failed.stdout[-4000:]
 for family,_,_,_,assertion in controls:
  match=re.search(r"thread 'actual_typescript_"+family+r"'[^\n]*panicked(.*?)(?=thread 'actual_typescript_|test actual_typescript_|failures:|\Z)",failed.stdout,re.S)
  assert match and assertion in match[0],(family,failed.stdout[-4000:])
 assert '8 failed' in failed.stdout,failed.stdout[-1500:]
finally:source.write_text(original)
passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
(out/'restored.log').write_text(passed.stdout)
assert passed.returncode==0 and '8 passed' in passed.stdout,passed.stdout[-4000:]
rows=[{'family':family,'function':function,'compiled_batch_exit':failed.returncode,'assertion':assertion,'restored_batch_exit':passed.returncode} for family,function,_,_,assertion in controls]
(out/'controls.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:print(row['family']+': compiled named failure (batch 101); restored 0')
