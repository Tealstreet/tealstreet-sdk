from pathlib import Path
import os,subprocess,json,re
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-phemex/src/start_order_formatting.rs'
out=root/'sdk-rs/target/phemex-start-order-evidence';out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('class','impl<V> ExchangeState<V>','("1m", "60")','("1m", "61")'),
 ('constructor','pub fn new_exchange','opts.clone(), 3.0','opts.clone(), 4.0'),
 ('start','pub async fn start','&u("tickers"), yes','&u("orders"), yes'),
 ('update','pub async fn update_order','h.boolean(false)','h.boolean(true)'),
 ('place','pub async fn place_order','get(h, &model, "orderID")','get(h, &model, "wrongID")'),
 ('format','pub async fn format_create_order','h.num((an - rest) / lots)','h.num((an - rest) / lots * 2.0)'),
 ('trailing','pub async fn format_create_trailing_stop_loss_order','h.num(-n)','h.num(n)'),
]
original=source.read_text();mutated=original
for family,marker,before,after in controls:
 start=mutated.index(marker);end=mutated.find('\npub ',start+1)
 if family=='class':end=mutated.index('\npub fn new_exchange',start)
 if family=='trailing':end=len(mutated)
 block=mutated[start:end if end!=-1 else len(mutated)];assert block.count(before)==1,(family,before,block.count(before));mutated=mutated[:start]+block.replace(before,after)+mutated[start+len(block):]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','start_order_formatting','--','--nocapture','--test-threads=1']
try:
 source.write_text(mutated)
 failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/'mutated.log').write_text(failed.stdout)
 assert failed.returncode==101 and 'Finished `test`' in failed.stdout,failed.stdout[-4000:]
 for family,_,_,_ in controls:
  match=re.search(r"thread 'actual_typescript_"+family+r"'[^\n]*panicked(.*?)(?=thread 'actual_typescript_|test actual_typescript_|failures:|\Z)",failed.stdout,re.S)
  assert match and ('ORDERS native individual effect' in match[0] or 'ORDERS native complete observation' in match[0]),(family,failed.stdout[-4000:])
 assert '7 failed' in failed.stdout,failed.stdout[-1500:]
finally:source.write_text(original)
passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/'restored.log').write_text(passed.stdout)
assert passed.returncode==0 and '7 passed' in passed.stdout,passed.stdout[-5000:]
rows=[{'family':f,'mutation':a+' -> '+b,'compiled_batch_exit':failed.returncode,'assertion':'ORDERS native decoded fields/effects','restored_batch_exit':passed.returncode} for f,_,a,b in controls];(out/'controls.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:print(row['family']+': compiled named failure (batch 101); restored 0')
