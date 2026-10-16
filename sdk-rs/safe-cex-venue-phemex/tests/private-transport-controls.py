from pathlib import Path
import os,subprocess,json,re
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-phemex'
out=root/'sdk-rs/target/phemex-private-transport-evidence';out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('dispose','private_session.rs','dispose','host.clear_symbols()?;','host.unset_timeout()?;','PRIVATE native complete observation'),
 ('schedule','private_session.rs','schedule_positions_and_balance_sync','host.debounce()?','host.debounce()? + 1.0','PRIVATE native effect schedule'),
 ('connect','private_session.rs','connect_and_subscribe','if host.testnet()?','if !host.testnet()?','PRIVATE native effect create'),
 ('open','private_session.rs','on_open','host.start_auth()?;','host.ping()?;','PRIVATE native effect ping'),
 ('ping','private_session.rs','ping','js_number_string(id)','js_number_string(id + 1.0)','PRIVATE native effect send'),
 ('auth','private_session.rs','auth','999.0','998.0','PRIVATE native effect send'),
 ('sync','private_session.rs','sync_positions_and_balance','host.update(balance, true, true)','host.update(balance, true, false)','PRIVATE native effect update'),
 ('api','api_transport.rs','create_api','host.retry(&client, 3, condition)','host.retry(&client, 4, condition)','PRIVATE native effect retry'),
]
originals={file:(venue/'src'/file).read_text() for _,file,*_ in controls}
mutated=dict(originals)
for family,file,function,before,after,assertion in controls:
 code=mutated[file];markers=['pub fn '+function+'<','pub async fn '+function+'<'];start=next(code.index(m) for m in markers if m in code)
 ends=[code.find('\npub ',start+1),code.find('\nfn ',start+1)];end=min((e for e in ends if e!=-1),default=len(code));block=code[start:end]
 assert block.count(before)==1,(family,before,block.count(before))
 mutated[file]=code[:start]+block.replace(before,after)+code[end:]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','private_transports','--','--nocapture','--test-threads=1']
try:
 for file,code in mutated.items():(venue/'src'/file).write_text(code)
 failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/'mutated.log').write_text(failed.stdout)
 assert failed.returncode==101 and 'Finished `test`' in failed.stdout,failed.stdout[-4000:]
 for family,_,_,_,_,assertion in controls:
  match=re.search(r"thread 'actual_typescript_"+family+r"'[^\n]*panicked(.*?)(?=thread 'actual_typescript_|test actual_typescript_|failures:|\Z)",failed.stdout,re.S)
  assert match and assertion in match[0],(family,failed.stdout[-4000:])
  assert 'family '+family+' case' in match[0],(family,match[0])
 assert '8 failed' in failed.stdout,failed.stdout[-1500:]
finally:
 for file,code in originals.items():(venue/'src'/file).write_text(code)
passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
(out/'restored.log').write_text(passed.stdout)
assert passed.returncode==0 and '8 passed' in passed.stdout,passed.stdout[-4000:]
rows=[{'family':family,'function':function,'compiled_batch_exit':failed.returncode,'assertion':assertion,'restored_batch_exit':passed.returncode} for family,_,function,_,_,assertion in controls]
(out/'controls.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:print(row['family']+': named compiled failure in independent family (batch 101); restored 0')
