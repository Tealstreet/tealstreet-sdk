from pathlib import Path
import os,subprocess,json,re
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-phemex/src/websocket_data.rs'
out=root/'sdk-rs/target/phemex-websocket-data-evidence';out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('public_class','impl<V> PublicSocket<V>','request_id: 1200.0','request_id: 1201.0'),
 ('private_class','impl<V> Default for PrivateSocket<V>','sync_debounce_ms: 1000.0','sync_debounce_ms: 999.0'),
 ('packed','pub fn map_packed_ticker_changes','* 100.0','* 101.0'),
 ('ticker','pub fn handle_ticker_stream_events','h.num(0.0)','h.num(1.0)'),
 ('private','pub fn private_on_message','h.string(u("aop_p.subscribe"))','h.string(u("aop_p.unsubscribe"))'),
 ('candles','impl<V: Clone + \'static> CandleSubscription<V>','for index in [1, 0]','for index in [0, 1]'),
 ('delta','pub fn process_order_book_update','h.parse(amount)?','h.parse(amount)? * 2.0'),
 ('book','async fn snapshot','h.num(1000.0)','h.num(999.0)'),
]
original=source.read_text();mutated=original
for family,marker,before,after in controls:
 start=mutated.index(marker);end=mutated.find('\npub ',start+1)
 if family=='public_class':end=mutated.index('\npub struct PrivateSocket',start)
 if family=='private_class':end=mutated.index('\npub fn map_packed_ticker_changes',start)
 if family=='candles':end=mutated.index('\npub fn process_order_book_update',start)
 if family=='book':end=len(mutated)
 block=mutated[start:end if end!=-1 else len(mutated)]
 assert block.count(before)==1,(family,before,block.count(before))
 mutated=mutated[:start]+block.replace(before,after)+mutated[start+len(block):]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','websocket_data','--','--nocapture','--test-threads=1']
try:
 source.write_text(mutated)
 failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/'mutated.log').write_text(failed.stdout)
 assert failed.returncode==101 and 'Finished `test`' in failed.stdout,failed.stdout[-4000:]
 for family,_,_,_ in controls:
  match=re.search(r"thread 'actual_typescript_"+family+r"'[^\n]*panicked(.*?)(?=thread 'actual_typescript_|test actual_typescript_|failures:|\Z)",failed.stdout,re.S)
  assert match and ('WS native complete observation' in match[0] or 'WS native individual effect' in match[0]),(family,failed.stdout[-4000:])
 assert '8 failed' in failed.stdout,failed.stdout[-1500:]
finally:source.write_text(original)
passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
(out/'restored.log').write_text(passed.stdout)
assert passed.returncode==0 and '8 passed' in passed.stdout,passed.stdout[-5000:]
rows=[{'family':f,'mutation':a+' -> '+b,'compiled_batch_exit':failed.returncode,'assertion':'WS native decoded values/effects','restored_batch_exit':passed.returncode} for f,_,a,b in controls]
(out/'controls.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:print(row['family']+': compiled named failure (batch 101); restored 0')
