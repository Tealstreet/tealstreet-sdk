"""Sequential, restored-on-exit controls against committed native source, not roundtrips."""
import json, os, pathlib, subprocess, sys
root=pathlib.Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-coinbase/src/portfolios_balances.rs'
original=source.read_text()
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
output=pathlib.Path(sys.argv[1]);output.mkdir(parents=True,exist_ok=True)
controls=[
 ('list-error-arguments','&[err.value.clone(), detail(&err)]','&[detail(&err)]','list-error: native error argument count'),
 ('base-input-order','replies.iter().position(|success| *success)','replies.iter().rposition(|success| *success)','base-first-input'),
 ('spot-probe-predicate','Some(429.0)','Some(430.0)','status-429: native probe predicate'),
 ('base-support','return Ok(Rc::new(RefCell::new(vec!["intx".into()])));','return Ok(Rc::new(RefCell::new(vec!["spot".into()])));','base-public: native supported types'),
 ('spot-support','Ok(Rc::new(RefCell::new(vec!["spot".into()])))','Ok(Rc::new(RefCell::new(vec!["intx".into()])))','spot-public-intx: native supported types'),
 ('balance-arithmetic','used: total - free,','used: free - total,','cfm-prefix: native number bits'),
 ('spot-currency-arithmetic','balance.borrow_mut().used = total - free;','balance.borrow_mut().used = free - total;','spot-keys-prototype: native number bits'),
 ('balance-composition','if has("spot") {','if has("spot") && false {','balance-cfm-spot: balance currency presence'),
]
env=dict(os.environ)
cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','portfolios_balances','--jobs','2','portfolio_balance_live_native_corpus','--','--exact']
results=[]
def run(name):
 p=subprocess.run(cmd,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (output/(name+'.log')).write_text(p.stdout);return p
try:
 for name,before,after,assertion in controls:
  assert before in original,(name,'missing mutation site')
  source.write_text(original.replace(before,after,1))
  p=run(name+'-broken'); source.write_text(original)
  q=run(name+'-restored')
  discriminated=p.returncode==101 and 'test portfolio_balance_live_native_corpus ... FAILED' in p.stdout and assertion in p.stdout and 'error[E' not in p.stdout
  results.append(dict(name=name,assertion=assertion,broken_exit=p.returncode,restored_exit=q.returncode,discriminated=discriminated))
  (output/'controls.json').write_text(json.dumps(dict(sha=sha,controls=results),indent=2)+'\n')
  print(json.dumps(results[-1]),flush=True)
  if not discriminated or q.returncode!=0:sys.exit(1)
finally:source.write_text(original)
