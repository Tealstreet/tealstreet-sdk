from pathlib import Path
import os,subprocess,json
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-phemex'
out=root/'sdk-rs/target/phemex-rest-account-order-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('context-margin','rest_account_control.rs','resolve_leverage_context','raw < 0.0','raw > 0.0','context','REST native result'),
 ('position-mode','rest_account_control.rs','change_position_mode','"Hedged"','"OneWay"','position_mode','REST native effect put'),
 ('margin-sign','rest_account_control.rs','change_margin_type','-leverage','leverage * 1.0','margin_type','REST native effect helper'),
 ('leverage-delegate','rest_account_control.rs','set_leverage','side, None','side, Some(true)','leverage_delegate','REST native effect helper'),
 ('leverage-sign','rest_account_control.rs','prep','safe * -1.0','safe * 1.0','leverage_helper','REST native effect put'),
 ('cancel-batch','rest_order_control.rs','cancel_orders','ids.chunks(10)','ids.chunks(9)','cancel_orders','REST native effect delete'),
 ('market-loaded','rest_order_control.rs','get_or_fetch_market','("markets", true)','("markets", false)','market_lookup','REST native effect update'),
]
results=[]
for name,file,function,before,after,test,assertion in controls:
 source=venue/'src'/file;original=source.read_text()
 markers=['pub fn '+function+'<','pub async fn '+function+'<','fn '+function+'<']
 start=next(original.index(m) for m in markers if m in original)
 endings=[original.find('\npub ',start+1),original.find('\nfn ',start+1)]
 end=min((e for e in endings if e!=-1),default=len(original))
 block=original[start:end];assert block.count(before)==1,(name,before,block.count(before))
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','rest_account_orders','actual_typescript_'+test,'--','--exact','--nocapture','--test-threads=1']
 try:
  source.write_text(original[:start]+block.replace(before,after)+original[end:])
  failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout,(name,failed.stdout[-2500:])
 finally:source.write_text(original)
 passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-2500:])
 results.append({'row_family':name,'compiled_exit':failed.returncode,'assertion':assertion,'restored_exit':passed.returncode})
 (out/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
assert len(results)==8
