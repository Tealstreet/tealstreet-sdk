"""Ten compiled controls for eight new rows. One Cargo process at a time."""
import hashlib,json,os,pathlib,subprocess
root=pathlib.Path(__file__).resolve().parents[1];repo=root.parent.parent
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
subprocess.run(['git','diff','--exit-code',sha,'--',str(root/'src'),str(root/'tests')],cwd=repo,check=True)
out=pathlib.Path('/tmp/coinbase-order-sync-controls')/sha;out.mkdir(parents=True,exist_ok=True)
controls=[
('cache-hit','src/cdp_signing.rs','r.cache.key_secret.borrow().as_str() == secret','r.cache.key_secret.borrow().as_str() != secret','signing_key_cache_races_and_pem_exception_identity','same-secret cached key root'),
('jwt-issuer','src/cdp_signing.rs','                    "cdp"','                    "wrong-issuer"','signing_native_claims_precede_wire_and_jwt','native issuer'),
('auth-id-read','src/exchange_ws_auth.rs','let result = async {\n                let id = prop(request.as_ref(), "id")?;','let result = async {','websocket_auth_native_own_slots_and_id_timing','native auth id captured at source time'),
('snapshot-id-replacement','src/order_sync.rs','by_id.insert(id, order);','by_id.entry(id).or_insert(order);','snapshot_pagination_native_orders_and_fetch_error_identity','.amount: native numeric bits'),
('fetch-error-carrier','src/order_sync.rs','                    Err(e)\n','                    Err(Rc::new(PortfolioThrown { value:e.value.clone() }))\n','snapshot_pagination_native_orders_and_fetch_error_identity','fetch wrapper rethrows original failure'),
('private-filled','src/order_sync.rs','                filled,\n                remaining,','                filled: remaining,\n                remaining,','private_event_native_values_before_store_and_fill','.filled: native numeric bits'),
('market-product-forwarding','src/order_sync.rs','host.resolve_coinbase_market(product_id, markets)','host.resolve_coinbase_market({let _=product_id; "BTC-USD".into()}, markets)','market_resolver_retains_map_product_and_selected_root','resolver forwards requested product'),
('detached-loop','src/exchange_tick.rs','let _detached_loop = r.host.loop_again(callback, delay)?;','let _detached_loop = r.host.loop_again(callback, delay)?.await?;','polling_tick_native_roots_disposal_callbacks_and_detached_loop','detached loop rejection must not reject tick'),
('fixture-content','tests/fixtures/order-sync-auth.json','ed jwt no uri','ed jwt no url','corpus_is_content_pinned_and_independently_counted','full corpus content fingerprint'),
('scheduled-callback','src/exchange_tick.rs','Rc::new(move || tick(next.clone()))','Rc::new(move || {let _retained=&next; Promise::resolved(Ok(()))})','polling_tick_native_roots_disposal_callbacks_and_detached_loop','normal tick without order poll: source trace unconsumed'),
]
assert len(controls)==10
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
for name,relative,old,new,test,assertion in controls:
 path=root/relative;original=path.read_bytes();text=original.decode();assert text.count(old)==1,(name,text.count(old))
 mutated_text=text.replace(old,new)
 if name=='auth-id-read':
  extra='let signature = r.host.sign_local(key, secret)?.await?;';assert mutated_text.count(extra)==1
  mutated_text=mutated_text.replace(extra,extra+'\n                let id = prop(request.as_ref(), "id")?;')
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','order_sync_auth',test,'--jobs','2','--','--exact']
 def run(kind):
  log=out/(name+'-'+kind+'.log')
  with log.open('w') as f:p=subprocess.run(cmd,cwd=repo,env=env,stdout=f,stderr=subprocess.STDOUT)
  return dict(exit=p.returncode,output=log.read_text(),log=str(log))
 try:path.write_text(mutated_text);mutated=run('mutated')
 finally:path.write_bytes(original)
 restored=run('restored');assert path.read_bytes()==original
 caught=mutated['exit']==101 and f'test {test} ... FAILED' in mutated['output'] and assertion in mutated['output'] and 'panicked at' in mutated['output']
 passed=restored['exit']==0 and f'test {test} ... ok' in restored['output']
 rec=dict(name=name,test=test,assertion=assertion,call_site=relative,old=old,new=new,original_sha256=hashlib.sha256(original).hexdigest(),mutated=mutated,restored=restored,caught=caught,restored_pass=passed)
 records.append(rec);(out/'results.json').write_text(json.dumps(dict(sha=sha,iteration='stopped after ten compiled mutations: eight new row families plus scheduled callback invocation and same-length fixture content replacement; each filtered test independently checks corpus content and 56-case floors',controls=records),indent=2)+'\n')
 print(json.dumps(dict(name=name,mutated_exit=mutated['exit'],restored_exit=restored['exit'],caught=caught,restored_pass=passed)),flush=True)
 if not caught or not passed:
  print(mutated['output'][-4000:]);print(restored['output'][-2500:]);raise SystemExit('SURVIVOR or invalid gate: see recorded call site')
subprocess.run(['git','diff','--exit-code',sha,'--',str(root/'src'),str(root/'tests')],cwd=repo,check=True)
