"""Run one actual-TS corpus per target, with compiling named controls and restoration."""
from pathlib import Path
import os,subprocess,argparse
parser=argparse.ArgumentParser();parser.add_argument('--start-at',type=int,default=0);parser.add_argument('--only');parser.add_argument('--check-anchors',action='store_true');args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/account_auth.rs';oracle=crate/'tests/account_auth_oracle.cjs'
env=dict(os.environ)
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH',subprocess.check_output(['node','-p',"require.resolve('typescript')"],cwd=crate.parent,text=True).strip())
cmd=['cargo','test','-p','safe-cex-venue-breakoutprop','--test','account_auth','--jobs','1']
controls=[
 (source,'token retained','*token = None;','*token = Some("stale".into());','INVALIDATE-NATIVE token cleared'),
 (source,'expiry stale','*expires_at = 0.0;','*expires_at = 1.0;','INVALIDATE-NATIVE expiry positive zero'),
 (source,'validation log msg swap','d.get("message")','d.get("msg")','VALIDATE-LOG message differs from msg'),
 (source,'validation fallback','Invalid API key or secret','wrong fallback','VALIDATE-NATIVE error object'),
 (source,'account filter','strict_equals(&Js::str("FULL_TRADING"))','strict_equals(&Js::str("DISABLED"))','ACCOUNT-NATIVE selected account'),
 (source,'account last match','let selected = valid.call_method(','let selected = Js::array(valid.array_values().expect("filtered accounts").into_iter().rev().collect()).call_method(','ACCOUNT-NATIVE selected account'),
 (source,'cache omission','*cache.borrow_mut() = Some(account.clone());','*cache.borrow_mut() = None;','ACCOUNT-CACHE committed value'),
 (source,'account metadata flattened','        result\n','        result.map_err(|e| {let mut flat=ExchangeError::new(e.message);flat.name=e.name;flat})\n','ACCOUNT-ERROR original metadata'),
 (oracle,'source candidate removal',"  const ex=new BreakoutpropExchange('breakoutprop',options,store())","  if(name==='multi-first')input.userDetails[0].accounts.pop();\n  const ex=new BreakoutpropExchange('breakoutprop',options,store())",'ACCOUNT-SOURCE candidate count'),
]
for p,name,old,new,assertion in controls[args.start_at:]:
 if args.only and name!=args.only:continue
 original=p.read_bytes();s=original.decode()
 assert s.count(old)==1,(name,s.count(old))
 if args.check_anchors:print(f'{name}: one production anchor');continue
 try:
  s=s.replace(old,new)
  p.write_text(s)
  r=subprocess.run(cmd,cwd=crate.parent,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: compiled; exit101; {assertion}',flush=True)
 finally:p.write_bytes(original)
 r=subprocess.run(cmd,cwd=crate.parent,env=env,capture_output=True,text=True);assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)
