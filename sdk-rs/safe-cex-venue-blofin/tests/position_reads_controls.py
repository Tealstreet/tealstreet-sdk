"""Serial production controls: named runtime failure, byte restore, named pass."""
import pathlib,subprocess,os,sys,json,re
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('execute','if !yielded {','if false && !yielded {','native_fetch_position_metadata','immediate_mode_dispatch'),
 ('fetch_positions_helper','"POSITIONS", params','"BALANCE", params','native_fetch_positions_helper'),
 ('fetch_positions','host.auth_failure_streak.get() > 0.0','host.auth_failure_streak.get() >= 0.0','native_fetch_positions'),
 ('fetch_position_metadata','symbols.chunks(20)','symbols.chunks(19)','native_fetch_position_metadata'),
 ('fetch_balance_and_positions','subtract(total_collateral, used)','subtract(total_collateral, upnl)','native_fetch_balance_and_positions'),
 ('fetch_income_history','ts < s','ts > s','native_fetch_income_history'),
 ('fetch_income_history_range','start_time: Some(start_time)','start_time: Some(start_time * 0.0 + end_time)','native_fetch_income_history_range'),
 ('fetch_closed_position_rows','row_id(row, "historyId")','row_id(row, "positionId")','native_fetch_closed_position_rows'),
 ('fetch_historical_positions','account: &opts.account','account: ""','native_fetch_historical_positions'),
 ('fetch_closed_pnl','row_id(row, "historyId")','row_id(row, "positionId")','native_fetch_closed_pnl'),
 ('query','if !value.is_finite()','if false && !value.is_finite()','native_position_read_boundaries','nonfinite_query_refusal'),
 ('fetch_position_metadata','if field(row,"instId")?','if false && field(row,"instId")?','native_position_read_boundaries','raw_numeric_symbol_refusal'),
 ('fetch_historical_positions','contract_size: None','contract_size: Some(&mut scale)','native_fetch_historical_positions','journal_base_units'),
]
controls += [
 ('fetch_balance_and_positions','let mut maintenance = None','let mut maintenance = Some(0.0)','native_fetch_balance_and_positions','negative_zero_maintenance'),
 ('fetch_balance_and_positions','let mut pnl = None','let mut pnl = Some(0.0)','native_fetch_balance_and_positions','negative_zero_upnl'),
 ('fetch_balance_and_positions','let mut available = None','let mut available = Some(0.0)','native_fetch_balance_and_positions','negative_zero_collateral'),
 ('fetch_income_history','query(market_id.as_deref(), start, end, after.as_deref())','query(selected_market_id(store, symbol).flatten().as_deref(), start, end, after.as_deref())','native_fetch_income_history','reentrant_income_market'),
 ('fetch_closed_position_rows','query(market_id.as_deref(), opts.from, opts.to, after.as_deref())','query(selected_market_id(store, Some(&opts.symbol)).flatten().as_deref(), opts.from, opts.to, after.as_deref())','native_fetch_closed_position_rows','reentrant_raw_market'),
 ('fetch_closed_pnl','query(market_id.as_deref(), opts.from, opts.to, after.as_deref())','query(selected_market_id(store, Some(&opts.symbol)).flatten().as_deref(), opts.from, opts.to, after.as_deref())','native_fetch_closed_pnl','reentrant_pnl_market'),
]
assert len(controls)==19
begin=int(sys.argv[2]) if len(sys.argv)>2 else 0
assert 0<=begin<19
results=[]
for control in controls[begin:]:
 method,before,after,assertion=control[:4];label=control[4]if len(control)>4 else method;path=root/'sdk-rs/safe-cex-venue-blofin/src/position_reads.rs';original=path.read_bytes();s=original.decode();match=re.search(r'\b(?:pub )?(?:async )?fn '+method+r'(?:<|\()',s);assert match,method;start=match.start();match=re.search(r'\n(?:pub )?(?:async )?fn ',s[start+1:]);end=start+1+match.start()if match else len(s);part=s[start:end];assert before in part,(method,before);part=part.replace(before,after,1)
 if label.startswith('reentrant_'):part=part.replace('let params = query(', 'let params = {let _ = &market_id; query(',1).replace('after.as_deref())?;', 'after.as_deref())?};',1)
 if label=='journal_base_units':part=part.replace('let mut deps = BlofinPositionAdapterDeps {','let mut scale = |_: &str| 0.01; let mut deps = BlofinPositionAdapterDeps {',1)
 mutant=s[:start]+part+s[end:];cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','position_reads','--jobs','2',assertion,'--','--exact','--nocapture']
 try:path.write_text(mutant);broken=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(label+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(original)
 restored=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(label+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and 'test '+assertion+' ... FAILED' in broken.stdout;passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout;row=dict(sha=sha,method=method,label=label,assertion=assertion,call_site='sdk-rs/safe-cex-venue-blofin/src/position_reads.rs',broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed);results.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=19,started_at=begin+1,stopped_after=begin+len(results),applied_this_run=len(results),results=results),indent=2)+'\n');print(json.dumps(row),flush=True)
 if not caught or not passed:sys.exit('FINDING or harness failure '+label+'; bytes restored, see logs')
