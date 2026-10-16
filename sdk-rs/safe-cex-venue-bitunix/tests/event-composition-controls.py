from pathlib import Path
import os,subprocess,json,sys
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-bitunix'
out=root/'sdk-rs/target/bitunix-event-composition-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('public-state','composition.rs','new_public_socket','max_attempts: 2.0','max_attempts: 3.0','construction','COMPOSITION native state'),
 ('public-secondary','composition.rs','new_public_socket','secondary.unwrap_or_else(default_secondary)','{ let _ignored = secondary; default_secondary() }','construction','COMPOSITION native state'),
 ('private-state','composition.rs','new_private_socket','handled_pong: false','handled_pong: true','construction','COMPOSITION native state'),
 ('futures-state','composition.rs','new_futures_socket','max_attempts: 2.0','max_attempts: 3.0','construction','COMPOSITION native state'),
 ('exchange-state','composition.rs','new_exchange','enable_fake_full_tpsl: false','enable_fake_full_tpsl: true','construction','COMPOSITION native state'),
 ('exchange-rps','composition.rs','new_exchange','options, 5.0','options, 6.0','construction','EVENT native effect rateLimit'),
 ('history-dedupe','event_models.rs','fetch_historical_fills','*old = (key, timestamp, o(fill))','let _preserved = (key, timestamp, o(fill)); let _old = old;','fill_history','HISTORY native result'),
 ('private-pong','event_models.rs','on_private_message','now - last > 5000.0','now - last >= 5000.0','private_message','EVENT native effect pong'),
 ('login-retry','event_models.rs','retry_login','host.sleep(2000.0)','host.sleep(2001.0)','private_message','EVENT native effect sleep'),
 ('cancel-endpoint','order_cancellation.rs','cancel_orders','endpoint("CANCEL_ORDER_TP_SL")','endpoint("CANCEL_ORDER")','cancel_orders','EVENT native effect post'),
 ('cancel-recreation','order_cancellation.rs','cancel_orders','if canceled || (!keep_tp && !keep_sl)','if canceled || true','cancel_orders','EVENT native complete effects'),
 ('book-total','orderbook_subscription.rs','parse','level.total = total','level.total = total + 1.0','book_listener','EVENT native effect book'),
 ('book-timer','orderbook_subscription.rs','cancel','host.clear(self.timeout.take().unwrap())?','self.timeout.take().unwrap()','book_listener','EVENT native complete effects'),
]
results=json.loads((out/'controls.json').read_text()) if sys.argv[1:] else []
if sys.argv[1:]: controls=[c for c in controls if c[0] in sys.argv[1:]]
for name,file,func,before,after,test,assertion in controls:
 source=venue/'src'/file
 original=source.read_text()
 markers=['pub fn '+func+'<','pub fn '+func+'(','pub async fn '+func+'<']
 start=next(original.index(m) for m in markers if m in original)
 endings=[original.find('\npub ',start+1),original.find('\n    pub ',start+1)]
 end=min((e for e in endings if e!=-1),default=len(original))
 body=original[start:end]
 assert body.count(before)==1,(name,before,body.count(before))
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitunix','--jobs','2','--test','event_composition','actual_typescript_'+test,'--','--exact','--nocapture','--test-threads=1']
 try:
  source.write_text(original[:start]+body.replace(before,after)+original[end:])
  failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout,(name,failed.stdout[-2500:])
 finally: source.write_text(original)
 passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-2500:])
 results=[r for r in results if r['family']!=name]
 results.append({'family':name,'compiled_exit':failed.returncode,'assertion':assertion,'restored_exit':passed.returncode})
 (out/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
assert len(results)==13
