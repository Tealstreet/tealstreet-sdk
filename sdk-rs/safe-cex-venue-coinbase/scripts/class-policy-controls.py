from pathlib import Path
import subprocess,time,json,hashlib,os,argparse
parser=argparse.ArgumentParser(description="Six serial production controls for live class policy and signing")
parser.add_argument("--out-dir",type=Path,required=True)
args=parser.parse_args()
args.out_dir=args.out_dir.resolve()
args.out_dir.mkdir(parents=True,exist_ok=True)
os.chdir(Path(__file__).resolve().parents[3])
if not os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"):
 raise SystemExit("Set SAFE_CEX_TYPESCRIPT_PATH to the isolated TypeScript compiler; preserve the oracle dependency environment.")
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert not subprocess.check_output(['git','diff','--name-only'],text=True).strip()
root=Path('sdk-rs/safe-cex-venue-coinbase')
policy='class_policy_reads_live_slots_keeps_captured_base_bodies_and_retains_native_promise_roots'
portfolio=Path('sdk-rs/safe-cex-venue-coinbase/src/portfolios_balances.rs').read_text()
live_body=portfolio[portfolio.index('pub fn get_supported_types<'):portfolio.index('pub fn get_spot_supported_types<')]
spot_read='r.class\n                .as_ref()\n                .map_or_else(|| r.support_spot.get(), |class| class.support_spot())'
assert live_body.count(spot_read)==1
stale_body=live_body.replace('Box::pin(async move {','Box::pin(async move { let captured_support_spot = '+spot_read+';',1).replace('if '+spot_read,'if captured_support_spot',1)
controls=[
 ('signing-lazy-and','sdk-rs/safe-cex-exchange/src/exchange.rs','options.key_truthy()\n        && (options.secret_truthy()','options.key_truthy()\n        & (options.secret_truthy()','signing_policy','native_signing_policy_observes_lazy_source_operand_order','native signing getter reads only the next source operand'),
 ('public-policy-callback-order','sdk-rs/safe-cex-exchange/src/exchange.rs','public_data_only_is_true() || !can_sign()','!can_sign() || public_data_only_is_true()','signing_policy','native_public_policy_preserves_lazy_virtual_signer_callback','source short circuit must skip virtual canSign'),
 ('class-key-policy','sdk-rs/safe-cex-venue-coinbase/src/classes.rs','class_option_truthy(self.0.options.borrow().get("key"))','class_option_truthy(self.0.options.borrow().get("missing-key-control"))','completion',policy,'class native signing policy selects ordinary portfolio'),
 ('live-support-after-pending-selection','sdk-rs/safe-cex-venue-coinbase/src/portfolios_balances.rs',live_body,stale_body,'completion',policy,'class native supportSpot is reread after pending selection'),
 ('current-spot-supported-body','sdk-rs/safe-cex-venue-coinbase/src/classes.rs','crate::portfolios_balances::get_spot_supported_types(','crate::portfolios_balances::get_supported_types(','completion',policy,'class native current supported arrow installs Spot body'),
 ('captured-base-current-finder','sdk-rs/safe-cex-venue-coinbase/src/classes.rs','        self.exchange.find_selected()','        crate::portfolios_balances::find_selected_portfolio(self.exchange.portfolio_receiver(self.exchange.host.clone(), false))','completion',policy,'class native HTTP call-site content'),
]
expected=['signing-lazy-and','public-policy-callback-order','class-key-policy','live-support-after-pending-selection','current-spot-supported-body','captured-base-current-finder']
assert [item[0] for item in controls]==expected
rows=[]
env=dict(os.environ,CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2')
sha=lambda data:hashlib.sha256(data).hexdigest()
for name,path,old,new,target,test,assertion in controls:
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==revision
 p=Path(path);original=p.read_bytes();assert original.decode().count(old)==1,(name,original.decode().count(old));mutant=original.decode().replace(old,new).encode()
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--locked','--jobs','2','--test',target,test,'--','--exact','--nocapture']
 print('START',name,flush=True)
 def run(label):
  start=time.time();log=args.out_dir/(name+'-'+label+'.log')
  with log.open('wb')as out:r=subprocess.run(command,stdout=out,stderr=subprocess.STDOUT,env=env)
  return {'exit':r.returncode,'elapsed_seconds':round(time.time()-start,2)},log.read_text(errors='replace')
 try:
  p.write_bytes(mutant);negative,output=run('negative')
 finally:p.write_bytes(original)
 restored,restored_output=run('restored')
 caught=negative['exit']==101 and assertion in output and 'test result: FAILED.' in output and ('test '+test+' ... FAILED')in output
 rows.append({'name':name,'code_revision':revision,'call_site':path,'named_assertion':assertion,'test':test,'command':command,'original_sha256':sha(original),'mutation_sha256':sha(mutant),'restored_sha256':sha(p.read_bytes()),'negative':negative,'restored':restored,'caught':caught})
 (args.out_dir/'controls.json').write_text(json.dumps({'code_revision':revision,'independent_control_ids':expected,'records':rows,'survivors':[r['name'] for r in rows if not r['caught']],'iteration':'Stopped after the six literal controls, not an exhaustive enumeration.'},indent=2)+'\n')
 print('END',name,negative['exit'],restored['exit'],caught,flush=True)
 assert p.read_bytes()==original
 if not caught or restored['exit']!=0:raise SystemExit(1)
assert not subprocess.check_output(['git','diff','--name-only'],text=True).strip()
