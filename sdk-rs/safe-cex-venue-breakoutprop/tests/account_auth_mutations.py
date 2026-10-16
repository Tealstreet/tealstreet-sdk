"""Run one actual-TS corpus per target, with compiling named controls and restoration."""
from pathlib import Path
import os,subprocess,argparse
from decoder_mutations import replace_decoder_field
parser=argparse.ArgumentParser();parser.add_argument('--start-at',type=int,default=0);args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/account_auth.rs';oracle=crate/'tests/account_auth_oracle.cjs';types=crate/'src/types.rs'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','account_auth','--jobs','1']
controls=[
 (source,'token retained','*token = None;','*token = Some("stale".into());','INVALIDATE-NATIVE token cleared'),
 (source,'expiry stale','*expires_at = 0.0;','*expires_at = 1.0;','INVALIDATE-NATIVE expiry positive zero'),
 (source,'validation log msg swap','d.get("message")','d.get("msg")','VALIDATE-LOG message differs from msg'),
 (source,'validation fallback','Invalid API key or secret','wrong fallback','VALIDATE-NATIVE error object'),
 (source,'account filter','a.account_status == "FULL_TRADING"','a.account_status == "DISABLED"','ACCOUNT-NATIVE selected account'),
 (source,'account last match','.find(|a| a.account.contains(&subaccount))','.rev().find(|a| a.account.contains(&subaccount))','ACCOUNT-NATIVE selected account'),
 (source,'cache omission','*cache.borrow_mut() = Some(selected.account.clone());','*cache.borrow_mut() = None;','ACCOUNT-CACHE committed value'),
 (source,'account metadata flattened','        result\n','        result.map_err(|e| {let mut flat=ExchangeError::new(e.message);flat.name=e.name;flat})\n','ACCOUNT-ERROR original metadata'),
 (oracle,'source candidate removal',"  const ex=new BreakoutpropExchange('breakoutprop',options,store())","  if(name==='multi-first')input.userDetails[0].accounts.pop();\n  const ex=new BreakoutpropExchange('breakoutprop',options,store())",'ACCOUNT-SOURCE candidate count'),
]
for field,ty in [('user_details','UserDetail'),('accounts','UserAccount')]:
 controls.append((types,'consuming '+field,f'    pub {field}: Vec<{ty}>,',f'    #[serde(deserialize_with="consume_all")]\n    pub {field}: Vec<{ty}>,','ACCOUNT-ERROR message'))
for p,name,old,new,assertion in controls[args.start_at:]:
 original=p.read_bytes();s=original.decode()
 if p!=types:assert s.count(old)==1,(name,s.count(old))
 try:
  s=replace_decoder_field(s,old,new) if p==types else s.replace(old,new)
  if p==types:s+='\nfn consume_all<\'de,D:serde::Deserializer<\'de>,T:serde::Deserialize<\'de>>(d:D)->Result<Vec<T>,D::Error>{let consumed=Vec::<T>::deserialize(d)?;drop(consumed);Ok(vec![])}\n'
  p.write_text(s)
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: compiled; exit101; {assertion}',flush=True)
 finally:p.write_bytes(original)
 r=subprocess.run(cmd,env=env,capture_output=True,text=True);assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)
