"""One compound control, ten mutations, eleven named assertions; serial byte restoration."""
import json,os,pathlib,subprocess,sys
out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();path=pathlib.Path('sdk-rs/safe-cex-venue-blofin/src/public_stream_data.rs');saved=path.read_bytes();source=saved.decode()
controls=[
 ('constructor',['native_constructor_close_capture'],[('(self.base_on_close)(event).map_err(PublicDataError::Host)?;','let _ = (&self.base_on_close, event);')]),
 ('dispose',['native_dispose_timer_order'],[('self.host.super_dispose().map_err(PublicDataError::Host)','Ok(())')]),
 ('dispatch',['native_dispatch_and_parser_boundaries'],[('cb(json.clone())?;','cb(Rc::new(RefCell::new(json.borrow().clone())))?;')]),
 ('ticker',['native_ticker_patch'],[('("volume", float(update.get("volCurrency24h"))?),','("volume", float(update.get("vol24h"))?),')]),
 ('funding',['native_funding_patch'],[('patch.set_number("fundingTime", float(update.get("fundingTime"))?);','patch.set_number("fundingTime", float(update.get("fundingTime"))? / 1000.0);')]),
 ('candle',['native_candle_listener'],[('if same {','if false && same {')]),
 ('book',['native_book_listener'],[('|| prev != state.last_seq','|| false')]),
 ('retry',['native_listener_retry_cleanup'],[('                100.0,','                10.0,')]),
 ('facade',['native_public_facade_composition'],[('core::on_open(&self.core, self.host.as_ref()).map_err(from_core)','self.ping()')]),
 ('ownership',['native_callback_ownership','native_listener_has_no_facade_self_cycle'],[('let weak = Rc::downgrade(&stream);','let kept = stream.clone();'),('if let Some(s) = weak.upgrade() {','if let Some(s) = Some(kept.clone()) {')]),
]
assert len(controls)==10
for label,_,changes in controls:
 for pattern,_ in changes:assert source.count(pattern)==1,label+': unique mutation target'
if len(sys.argv)>2 and sys.argv[2]=='--preflight':print('10 mutations/11 named assertions; unique targets; no Cargo or mutation');raise SystemExit(0)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
# One compound control: all ten mutations applied together. This proves that all
# eleven named tests discriminate, not that each mutation is independently caught.
expected=[test for _,tests,_ in controls for test in tests];assert len(expected)==11
changed=source
for _,_,changes in controls:
 for pattern,replacement in changes:changed=changed.replace(pattern,replacement,1)
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','public_stream_data','--jobs','2','--','--nocapture']
try:
 path.write_text(changed)
 with(out/'compound-broken.log').open('w')as f:broken=subprocess.run(command,env=env,stdout=f,stderr=subprocess.STDOUT)
finally:path.write_bytes(saved)
with(out/'compound-restored.log').open('w')as f:restored=subprocess.run(command,env=env,stdout=f,stderr=subprocess.STDOUT)
broken_text=(out/'compound-broken.log').read_text();restored_text=(out/'compound-restored.log').read_text()
failed=[test for test in expected if f'test {test} ... FAILED' in broken_text];passed=[test for test in expected if f'test {test} ... ok' in restored_text]
record=dict(sha=sha,command=command,control_kind='one compound control',applied_mutations=10,expected_named_assertions=11,stopped_after_compound_control=1,broken_exit=broken.returncode,restored_exit=restored.returncode,failed=failed,passed=passed,surviving_named_tests=[t for t in expected if t not in failed],individual_mutation_discrimination='not measured by this compound control',call_site=str(path),mutations=controls)
(out/'results.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)
assert path.read_bytes()==saved
assert broken.returncode==101 and 'panicked at' in broken_text and len(failed)==11,'Named assertion survivor/compiler-only failure; retained finding in results.json'
assert restored.returncode==0 and len(passed)==11 and '11 passed; 0 failed' in restored_text,'Restoration failure; retained results.json'
