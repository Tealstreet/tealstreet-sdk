"""Compiled mutation floor for the eight new rows; never run Cargo concurrently."""
import hashlib,json,os,pathlib,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[1];repo=root.parent.parent
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
out=pathlib.Path('/tmp/coinbase-public-market-controls')/sha;out.mkdir(parents=True,exist_ok=True)
read_test='public_reads_assert_native_catalog_candles_and_query_bits_against_live_source'
listener_test='public_listeners_assert_native_records_book_levels_and_retained_roots_against_live_source'
controls=[
 ('catalog-contract','src/public_market_reads.rs','                        contract,','                        contract + 1.0,',read_test),
 ('intx-filter','src/public_market_reads.rs','Some("PERP")','Some("SPOT")',read_test),
 ('historical-reverse','src/public_market_reads.rs','            candles.reverse();','            // mutation: do not reverse',read_test),
 ('data-spread','src/public_data.rs','.iter()\n                .map(|(k, v)| (k.clone(), Some(v.clone())))','.iter()\n                .filter(|(k, _)| !k.iter().copied().eq("marker".encode_utf16()))\n                .map(|(k, v)| (k.clone(), Some(v.clone())))',listener_test),
 ('candle-trade-volume','src/public_market_listeners.rs','.add(current.get("volume"), volume / price)?','.add(current.get("volume"), volume)?',listener_test),
 ('book-snapshot-side','src/public_market_listeners.rs','let levels = if side == "asks"','let levels = if side == "bids"',listener_test),
 ('candle-delegate','src/public_market_listeners.rs','host.public_listen_ohlcv(opts, callback)','host.public_listen_ohlcv(Rc::new(RefCell::new(opts.borrow().clone())), callback)','delegates_and_listener_registration_preserve_native_roots'),
 ('book-delegate','src/public_market_listeners.rs','host.public_listen_order_book(symbol, callback)','host.public_listen_order_book(format!("{symbol}-MUTATED"), callback)','delegates_and_listener_registration_preserve_native_roots'),
 ('fixture-same-length-replacement','tests/fixtures/public-market-data.json','"catalog spot intx cfm"','"catalog swap intx cfm"',read_test),
]
# Nine deliberate compiled controls, stopped here. This is a floor, not exhaustive mutation coverage.
assert len(controls)==9
records=[]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
for name,relative,old,new,test in controls:
 path=root/relative;original=path.read_bytes();text=original.decode();assert text.count(old)==1,(name,text.count(old))
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','public_market_data',test,'--','--exact']
 cmd[cmd.index('--') : cmd.index('--')]=['--jobs','2']
 def run(kind):
  log=out/(name+'-'+kind+'.log')
  with log.open('w') as f:p=subprocess.run(cmd,cwd=repo,env=env,stdout=f,stderr=subprocess.STDOUT)
  return dict(exit=p.returncode,output=log.read_text(),log=str(log))
 try:
  path.write_text(text.replace(old,new));mutated=run('mutated')
 finally:path.write_bytes(original)
 restored=run('restored');assert path.read_bytes()==original
 caught=mutated['exit']==101 and f'test {test} ... FAILED' in mutated['output'] and 'panicked at' in mutated['output']
 passed=restored['exit']==0 and f'test {test} ... ok' in restored['output']
 rec=dict(name=name,test=test,call_site=relative,old=old,new=new,original_sha256=hashlib.sha256(original).hexdigest(),mutated=mutated,restored=restored,caught=caught,restored_pass=passed)
 records.append(rec);(out/'results.json').write_text(json.dumps(dict(sha=sha,iteration='stopped after nine controls covering Data/catalog/INTX/history/candles/books/both delegates and same-length fixture replacement',controls=records),indent=2)+'\n')
 print(json.dumps(dict(name=name,mutated_exit=mutated['exit'],restored_exit=restored['exit'],caught=caught,restored_pass=passed)),flush=True)
 if not caught or not passed:
  print(mutated['output'][-4000:]);print(restored['output'][-2000:]);raise SystemExit('SURVIVOR or invalid mutation gate: see recorded call site')
