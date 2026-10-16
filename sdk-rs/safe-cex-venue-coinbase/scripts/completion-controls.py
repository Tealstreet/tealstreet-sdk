"""One compiled negative/restore gate per remaining Coinbase row family.
The declared class controls prove owned initial state, not absent callback coupling.
"""
import hashlib,json,os,pathlib,subprocess,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[3]
os.chdir(ROOT)
CRATE=pathlib.Path('sdk-rs/safe-cex-venue-coinbase')
CARGO='/opt/homebrew/opt/rustup/bin/cargo'
ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
subprocess.run(['git','diff','--exit-code',sha,'--',str(CRATE/'src'),str(CRATE/'tests')],check=True)

def replace_last(text,needle,replacement):
 i=text.rindex(needle);return text[:i]+replacement+text[i+len(needle):]

def replace_once(text,needle,replacement):
 assert text.count(needle)==1,(needle,text.count(needle));return text.replace(needle,replacement)

controls=[
 ('API factory retries','api_factory.rs','api_native_fields_aliases_branch_order_and_async_headers','native retries',lambda s:replace_once(s,'r.host.retry(client.clone(), 3.0, condition)?','r.host.retry(client.clone(), 4.0, condition)?')),
 ('API public interceptor header constructor','api_factory.rs','api_native_fields_aliases_branch_order_and_async_headers','native public AxiosHeaders.from branch',lambda s:replace_once(s,'r.host.headers(content(), true)?','r.host.headers(content(), false)?')),
 ('native edit amended PRICE precision swap','order_edit.rs','edit_native_payload_precision_preflight_and_error_identity','native edit price respects price precision',lambda s:replace_last(s,'market.borrow().precision.price,','market.borrow().precision.amount,')),
 ('native edit amended AMOUNT precision swap','order_edit.rs','edit_native_payload_precision_preflight_and_error_identity','native edit size respects amount precision',lambda s:replace_last(s,'market.borrow().precision.amount,','market.borrow().precision.price,')),
 ('constructor secondary rate limit','classes.rs','classes_native_initializers_constructor_order_and_shared_slots','native constructor rate limit three',lambda s:replace_once(s,'rate_host.rate_limit(client, 3.0)','rate_host.rate_limit(client, 4.0)')),
 ('exchange selected slot copied instead of shared','classes.rs','classes_native_initializers_constructor_order_and_shared_slots','native exchange projections share selected portfolio slot',lambda s:s.replace('selected: self.selected_portfolio.clone(),','selected: Rc::new(RefCell::new(self.selected_portfolio.borrow().clone())),',1)),
 ('public class heartbeat initial count','classes.rs','classes_native_initializers_constructor_order_and_shared_slots','native public heartbeat initial subscription count',lambda s:replace_once(s,'PublicStreamReceiver::new(host)','{ let r = PublicStreamReceiver::new(host); r.topics.borrow_mut().insert("heartbeats".into(), 2.0); r }')),
 ('private class subscription initial attempt','classes.rs','classes_native_initializers_constructor_order_and_shared_slots','native private subscription initial attempt',lambda s:replace_once(s,'subscription_attempt: Cell::new(0.0)','subscription_attempt: Cell::new(1.0)')),
 ('spot subclass own support field','classes.rs','classes_native_initializers_constructor_order_and_shared_slots','native spot support field',lambda s:replace_once(s,'exchange.support_spot.set(true)','exchange.support_spot.set(false)')),
]
assert len(controls)==9 # Independent chosen control floor, not expected from a loop.
records=[]
output=CRATE/'COMPLETION_CONTROLS.json'

def save():
 output.write_text(json.dumps(dict(tested_sha=sha,iteration='Stopped after these nine explicit controls. No exhaustive mutation claim.',records=records,survivors=[r['name'] for r in records if not r['caught']],proof_limit='Class controls discriminate real owned defaults/projections only. Four class callback wiring FINDING rows remain divergent.'),indent=2)+'\n')

def run(cmd):
 start=time.monotonic();p=subprocess.run(cmd,env=ENV,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 return dict(exit=p.returncode,seconds=round(time.monotonic()-start,2),output=p.stdout)

for name,file,test,assertion,mutate in controls:
 p=CRATE/'src'/file;baseline=p.read_bytes();mutated=mutate(baseline.decode()).encode();assert baseline!=mutated
 cmd=[CARGO,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','completion','--jobs','2',test,'--','--exact']
 print('CONTROL',name,flush=True)
 try:
  p.write_bytes(mutated);negative=run(cmd)
 finally:
  p.write_bytes(baseline)
 restored=run(cmd)
 caught=negative['exit']==101 and assertion in negative['output'] and 'test result: FAILED.' in negative['output']
 records.append(dict(name=name,call_site=str(p),named_assertion=assertion,command=cmd,source_sha256=hashlib.sha256(baseline).hexdigest(),mutation_sha256=hashlib.sha256(mutated).hexdigest(),negative=negative,restored=restored,caught=caught))
 save();print('RESULT',name,negative['exit'],restored['exit'],'caught',caught,flush=True)
 if not caught or restored['exit']!=0:
  print('FINDING or restore failure; preserved record at',output,flush=True);sys.exit(1)
subprocess.run(['git','diff','--exit-code',sha,'--',str(CRATE/'src'),str(CRATE/'tests')],check=True)
print('All nine named negative gates exit 101; all nine restores exit 0.',flush=True)
