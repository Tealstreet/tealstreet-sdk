"""One serial, restored production mutation per newly mapped mode utility row."""
import json, pathlib, subprocess, sys, os
root=pathlib.Path(__file__).resolve().parents[3]
out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
source=root/'sdk-rs/safe-cex-venue-blofin/src/modes.rs'
original=source.read_bytes()
controls=[
 ('getPositionSide','if options.side == OrderSide::Buy {','if options.side == OrderSide::Sell {','native_position_side_mode_precedence_flip_and_fetch_error'),
 ('applyConfirmedPositionMode','ids.insert(position.id.clone())','ids.insert(position.id.clone()) || true','modes::tests::native_prepared_mode_update_dedupes_before_store_normalization'),
 ('afterMethodCalledInWorker','Some("oneway") => PositionMode::OneWay','Some("oneway") => PositionMode::Hedged','native_worker_confirmed_result_fallback_margin_and_unknown_method'),
]
results=[]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
for row,before,after,assertion in controls:
 assert original.decode().count(before)==1,row
 target=['--lib'] if row=='applyConfirmedPositionMode' else ['--test','modes']
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin',*target,'--jobs','2',assertion,'--','--exact','--nocapture']
 try:
  source.write_text(original.decode().replace(before,after,1))
  broken=subprocess.run(command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (out/(row+'-broken.log')).write_text(broken.stdout)
 finally:source.write_bytes(original)
 restored=subprocess.run(command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (out/(row+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and ('test '+assertion+' ... FAILED') in broken.stdout
 passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout
 result=dict(row=row,assertion=assertion,call_site='safe-cex-venue-blofin/src/modes.rs',broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed)
 results.append(result);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=3,stopped_after=len(results),results=results),indent=2)+'\n');print(json.dumps(result),flush=True)
 if not caught or not passed:sys.exit('FINDING or harness failure '+row+'; source restored; see logs')
assert source.read_bytes()==original
