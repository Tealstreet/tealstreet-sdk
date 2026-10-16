from pathlib import Path
import os,subprocess,json,sys
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-phemex'
out=root/'sdk-rs/target/phemex-session-position-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('dispose-order','session_control.rs','dispose','"publicWebsocket"','"privateWebsocket"','dispose','PH native effect socketDispose'),
 ('private-setting','session_control.rs','start_private','set_setting("isHedged", true)','set_setting("isHedged", false)','private_start','PH native effect setting'),
 ('public-balance','session_control.rs','start_public_only','("used", 0.0)','("used", 1.0)','public_start','PH native effect update'),
 ('tick-loaded','session_control.rs','tick','vec!["balance", "positions"]','vec!["balance", "orders"]','tick','PH native effect update'),
 ('ohlcv-handler','session_control.rs','listen_ohlcv','"listenOHLCV"','"listenOrderBook"','ohlcv_delegate','PH native effect handler'),
 ('book-handler','session_control.rs','listen_order_book','"listenOrderBook"','"listenOHLCV"','book_delegate','PH native effect handler'),
 ('position-currency','position_control.rs','fetch_position_for_symbol','"USDC"','"USDT"','position_read','PH native effect get'),
 ('mode-match','position_control.rs','fetch_raw_position_mode','host.same_symbol(&formatted, &symbol)?','!host.same_symbol(&formatted, &symbol)?','mode_read','PH native effect formatIn'),
 ('bound-mapper','position_control.rs','fetch_position_for_symbol','host.map_position(&mapper, row)?','{ let _captured = &mapper; let live = host.bind_mapper()?; host.map_position(&live, row)? }','position_read','PH native effect bind'),
 ('required-info','position_control.rs','get_order_position_side','host.info_side(&options, false)','host.info_side(&options, true)','order_side','PH native optional then required info path'),
 ('order-side','position_control.rs','get_order_position_side','host.is_buy(&options)?','!host.is_buy(&options)?','order_side','PH native result'),
]
results=json.loads((out/'controls.json').read_text()) if sys.argv[1:] else []
if sys.argv[1:]: controls=[c for c in controls if c[0] in sys.argv[1:]]
for name,file,func,before,after,test,assertion in controls:
 source=venue/'src'/file;original=source.read_text()
 start=next(original.index(m) for m in ['pub fn '+func+'<','pub async fn '+func+'<'] if m in original)
 end=original.find('\npub ',start+1)
 if end==-1:end=len(original)
 body=original[start:end];assert body.count(before)==1,(name,before,body.count(before))
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','session_position','actual_typescript_'+test,'--','--exact','--nocapture','--test-threads=1']
 try:
  source.write_text(original[:start]+body.replace(before,after)+original[end:])
  failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout,(name,failed.stdout[-2400:])
 finally:source.write_text(original)
 passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-2400:])
 results=[r for r in results if r['family']!=name]
 results.append({'family':name,'compiled_exit':failed.returncode,'assertion':assertion,'restored_exit':passed.returncode})
 (out/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
assert len(results)==11
