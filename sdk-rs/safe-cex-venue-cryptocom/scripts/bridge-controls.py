"""Sequential, compiling runtime controls; restore every source mutation before continuing."""
import hashlib,json,os,shutil,subprocess,sys,tempfile,time
from pathlib import Path
root=Path(__file__).resolve().parents[3]
os.chdir(root)
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
crate=Path('sdk-rs/safe-cex-venue-cryptocom')
reference=Path('packages/safe-cex/src/exchanges/cryptocom')
gate_inputs=[str(crate/'src'),str(crate/'tests'),str(crate/'scripts'),str(reference)]
subprocess.run(['git','diff','--exit-code',sha,'--',*gate_inputs],check=True)
for path in sorted([*(crate/'src').glob('*.rs'),*(crate/'tests').glob('*'),*(crate/'scripts').glob('*.mjs'),*reference.glob('*.ts')]):
 if path.is_file():
  expected=subprocess.check_output(['git','show',sha+':'+str(path)])
  assert expected==path.read_bytes(),('uncommitted gate input',str(path))
evidence=crate/'evidence'/('bridge-controls-'+sha[:10]);evidence.mkdir(parents=True,exist_ok=True)
base=['cargo','test','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-cryptocom','--jobs','1','--lib']
controls=[
 ('series-reentry','native_bindings.rs','let callback = Rc::new(callback);','let callback = Rc::new(std::cell::RefCell::new(callback));','native_series_callback_factory_can_reenter','already borrowed'),
 ('iterator-completed-close','native_bindings.rs','Err(error) => Err(error),','Err(error) => Err((self.ops.close_throw)(record,error)),','native_destructuring_closes_only_active_binding_errors','NATIVE-ITERATOR-CLOSE-PHASE'),
 ('position-channel','class_socket.rs','"user.positions"','"user.position"','native_private_position_channel_is_plural','NATIVE-POSITION-CHANNEL-VALUE'),
 ('read-dispatch-swap','class_exchange.rs','"fetchMarkets" => Box::pin(read::fetch_markets(h.clone()))','"fetchMarkets" => Box::pin(read::fetch_tickers(h.clone()))','assembled_rest_dispatch_uses_distinct_native_endpoints','NATIVE-REST-DISPATCH-ENDPOINT'),
 ('exchange-noop','class_exchange.rs',None,None,'assembled_exchange_fields_reach_real_bodies','NATIVE-EXCHANGE-BINDING'),
 ('public-noop','class_socket.rs',None,None,'assembled_socket_fields_reach_real_bodies','NATIVE-SOCKET-BINDING'),
 ('method-receiver','native_bindings.rs','(self.ops.value.invoke)(prepared.0, prepared.1, args)','(self.ops.value.invoke)(prepared.0, (self.ops.value.undefined)(), args)','assembled_account_posts_eagerly_then_suspends_with_original_receiver','NATIVE-ACCOUNT-POST-RECEIVER'),
 ('await-turn-model','bridge_tests.rs','if !self.1 {','if false && !self.1 {','assembled_account_posts_eagerly_then_suspends_with_original_receiver','NATIVE-ACCOUNT-AWAIT-SUSPENDS'),
 ('enum-comparison','class_mappings.rs','(h.ops.value.equal)(&value, &test)','{ let _ = &value; (h.ops.value.equal)(&test, &test) }','assembled_mapping_reads_live_native_imports','NATIVE-MAPPING-NONMATCH-DEFAULT'),
 ('account-field-swap','class_admin.rs','"userId"','"affiliateId"','assembled_get_account_retains_decoded_native_uuid','NATIVE-ACCOUNT-UUID-IDENTITY'),
 ('parse-float-order','rest_reads.rs','let prepared = h.prepare_parse_float()?;\n    h.call_parse_float(prepared, h.get(value, key)?)','h.parse_float(h.get(value, key)?)','native_parse_float_captures_callee_before_order_field_getters','NATIVE-PARSE-FLOAT-CAPTURE'),
 ('parse-float-price-order','rest_reads.rs',None,None,'native_parse_float_captures_callee_before_order_field_getters','NATIVE-PARSE-FLOAT-CAPTURE'),
 ('order-side-import','rest_reads.rs','h.imported("OrderSide", "Buy")?','h.string("buy")','assembled_order_retains_imported_side_value','NATIVE-ORDER-SIDE-IMPORT'),
 ('position-side-fresh-read','rest_reads.rs','let side = h.imported("PositionSide", side)?;','let side = h.string(side);','assembled_positions_retain_each_live_enum_read','NATIVE-POSITION-ENUM-READ-COUNTS'),
 ('position-margin-import','rest_reads.rs','let margin = h.imported("MarginType", "Cross")?;','let margin = h.string("cross");','assembled_positions_retain_each_live_enum_read','NATIVE-POSITION-ENUM-READ-COUNTS'),
 ('socket-order-enum','socket_handlers.rs','get(h, &(h.global)("OrderSide")?, member)?','(h.literal)(if member == "Buy" { "buy" } else { "sell" })','socket_order_retains_selected_enum_after_side_getter','NATIVE-SOCKET-ORDER-SIDE-IDENTITY'),
 ('socket-position-enum','socket_handlers.rs','let side = get(&h, &(h.global)("PositionSide")?, side_member)?;','let side = (h.literal)(if side_member == "Long" { "long" } else { "short" });','socket_position_retains_side_and_margin_at_source_order','NATIVE-SOCKET-POSITION-ID-SIDE'),
 ('socket-margin-enum','socket_handlers.rs','get(&h, &(h.global)("MarginType")?, member)?','(h.literal)(if member == "Isolated" { "isolated" } else { "cross" })','socket_position_retains_side_and_margin_at_source_order','NATIVE-SOCKET-POSITION-MARGIN-IDENTITY'),
 ('public-constructor-binding','socket_classes.rs','crate::class_socket::member(h.binding.clone(), receiver.clone(), "onOpen", false)','(h.body.undefined)()','constructed_socket_fields_call_concrete_bodies','NATIVE-ASSEMBLED-SOCKET-FIELD'),
 ('private-constructor-binding','socket_classes.rs','receiver.clone(),\n        "subscribeToPositionUpdates",\n        true','receiver.clone(),\n        "ping",\n        true','constructed_socket_fields_call_concrete_bodies','NATIVE-ASSEMBLED-PRIVATE-CHANNEL-COUNT'),
 ('exchange-constructor-binding','runtime.rs','crate::class_exchange::member(h.binding.clone(), receiver.clone(), name)','{ if *name == "getAccount" { let undefined = h.value.undefined.clone(); (h.value.function)(name, 0, Rc::new(move |_| Ok(undefined()))) } else { crate::class_exchange::member(h.binding.clone(), receiver.clone(), name) } }','runtime:constructed_exchange_get_account_calls_concrete_body_with_lexical_receiver','NATIVE-ASSEMBLED-ACCOUNT-PROMISE'),
 ('proof-admission-guard','scripts/bridge-proof.mjs','export function validateBridgeProof(receipts, { sha, logs }) {',"export function validateBridgeProof(receipts, { sha, logs }) { assert.equal(receipts.length,46); return Object.freeze({ sha, controls:23, receipts:46, stop:'index22 source-noop' });",'proof_admission_requires_independent_control_pairs','BRIDGE-PROOF-GUARD'),
 ('source-noop',None,None,None,'source_body_entry_floor_matches_native_after_native_assertions','SOURCE-BRIDGE-ORACLE'),
]
records=[]
def run(label,name,expected,assertion,env):
 target=base[:-1]+['--test','runtime',name.split(':',1)[1]] if name.startswith('runtime:') else base+['native_bindings::tests::'+name]
 command=target+['--','--exact','--include-ignored','--nocapture','--test-threads=1']
 started=time.time();r=subprocess.run(command,env=env,capture_output=True,text=True);output=r.stdout+r.stderr
 for value,replacement in [(str(root),'<worktree>'),(str(Path.home()),'<home>'),(tempfile.gettempdir(),'<temporary-path>')]:output=output.replace(value,replacement)
 compiler=env.get('SAFE_CEX_TYPESCRIPT_PATH')
 if compiler:output=output.replace(compiler,'<external-typescript>')
 for tool in ['cargo','node',sys.executable]:
  resolved=shutil.which(tool)
  if resolved:output=output.replace(str(Path(resolved).resolve()),'<tool-executable>')
 filename=f'{len(records):02d}-{label}.log';(evidence/filename).write_text(output)
 runtime='running 1 test' in output and 'test result:' in output
 outcome=('SURVIVOR' if expected==101 and r.returncode==0 else 'COMPILER-BLOCKED' if 'could not compile' in output else 'CAUGHT' if expected==101 and r.returncode==101 and runtime and assertion in output else 'RESTORED-PASS' if expected==0 and r.returncode==0 and runtime else 'UNEXPECTED')
 record={'sha':sha,'label':label,'expected_exit':expected,'outcome':outcome,'runtime_executed':runtime,'mutation_file':str(path) if path else 'packages/safe-cex/src/exchanges/cryptocom/cryptocom.exchange.ts (in-memory getAccount)','input_sha256':hashlib.sha256(path.read_bytes()).hexdigest() if path else None,'command':command,'exit':r.returncode,'seconds':round(time.time()-started,2),'named_assertion':assertion,'log':filename}
 records.append(record);(evidence/'controls.json').write_text(json.dumps(records,indent=2)+'\n')
 assert r.returncode==expected,(label,r.returncode)
 assert 'running 1 test' in output,(label,'test did not execute')
 if expected:
  assert 'test result: FAILED' in output and assertion in output,(label,'missing runtime failure')
  assert 'could not compile' not in output,(label,'compilation failure is not mutation proof')
 else:assert 'test result: ok. 1 passed' in output,(label,'missing named restored success')
 print(label,'EXIT',r.returncode,flush=True)
for label,filename,old,new,test,assertion in controls:
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
 path=(crate/filename if filename.startswith('scripts/') else crate/'src'/filename) if filename else None
 original=path.read_bytes() if path else None
 env=os.environ.copy()
 print('CONTROL',label,sha,flush=True)
 failure=None
 try:
  if path:
   source=original.decode()
   if label=='series-reentry':
    assert source.count(old)==1 and source.count('let future = callback(value);')==1
    source=source.replace(old,new).replace('let future = callback(value);','let future = (callback.borrow_mut())(value);')
   elif label=='parse-float-price-order':
    declaration='    let parse_price = h.prepare_parse_float()?;\n'; call='h.call_parse_float(\n            parse_price,\n'
    assert source.count(declaration)==1 and source.count(call)==1
    source=source.replace(declaration,'').replace(call,'h.parse_float(\n')
   elif label=='account-field-swap':
    assert source.count(old)==1 and source.count(new)==1
    source=source.replace(old,'"__field_swap_control__"').replace(new,old).replace('"__field_swap_control__"',new)
   elif label in ['exchange-noop','public-noop']:
    marker='pub fn member';start=source.index(marker);before=source[:start];suffix=source[start:];needle='Rc::new(move |args| {';assert needle in suffix
    method='fetchMarkets' if label=='exchange-noop' else 'onOpen'
    source=before+suffix.replace(needle,needle+' if name=="'+method+'" {return Ok((h.ops.value.undefined)());}',1)
   else:
    assert source.count(old)==1,(label,source.count(old));source=source.replace(old,new)
   assert source!=original.decode();path.write_text(source)
  else:env['SAFE_CEX_CRYPTOCOM_BRIDGE_SOURCE_MUTATION']='getAccount'
  try:run(label+'-mutated',test,101,assertion,env)
  except Exception as error:failure=error
 finally:
  if path:path.write_bytes(original)
 env.pop('SAFE_CEX_CRYPTOCOM_BRIDGE_SOURCE_MUTATION',None)
 run(label+'-restored',test,0,None,env)
 if failure is not None:raise failure
 subprocess.run(['git','diff','--exit-code',sha,'--',*gate_inputs],check=True)
assert len(records)==46
print('COMPLETE floor23 stopped index22 source-noop; host await-turn control is a witness-model control.',flush=True)
