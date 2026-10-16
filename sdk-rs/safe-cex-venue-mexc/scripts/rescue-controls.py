"""Fresh production mutations for the three inherited PR7424 venue rows."""
from pathlib import Path
import json,os,subprocess
crate=Path(__file__).resolve().parents[1]
rust=crate/'src/exchange.rs'
logs=Path(os.environ['MEXC_RESCUE_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],cwd=crate,text=True).strip(),'commit lane files before binding gates to HEAD'
controls=[
 ('validation-success','exchange::tests::mexc_account_identity_matches_live_source','MEXC account validation result','error: AccountValidationError::from(String::new()),','error: AccountValidationError::from("unexpected"),'),
 ('account-hash','exchange::tests::mexc_account_identity_matches_live_source','MEXC account identity','digest[..12]','digest[..11]'),
 ('cancel-status','exchange::tests::mexc_cancel_orders_matches_live_source','cancelOrders native status:','.update_orders_status(&ids, OrderStatus::Canceled)','.update_orders_status(&ids, OrderStatus::Open)'),
]
assert len(controls)==3,'MEXC rescue control floor'
assert controls[-1][0]=='cancel-status','MEXC rescue last iteration'
original=rust.read_text()
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=crate,text=True).strip(),'floor':'three row controls, stopped after cancel-status','controls':[]}
def run(name,test):
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--lib','--locked','--jobs','2','--','--exact',test,'--nocapture']
 result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
 return result
for name,test,assertion,old,new in controls:
 assert original.count(old)==1,old
 baseline=run(name+'-baseline',test);assert baseline.returncode==0,baseline.stdout
 try:
  rust.write_text(original.replace(old,new))
  mutated=run(name+'-mutation',test)
  caught=mutated.returncode==101 and assertion in mutated.stdout
 finally:rust.write_text(original)
 restored=run(name+'-restored',test)
 summary['controls'].append({'name':name,'test':test,'baseline_exit':baseline.returncode,'mutation_exit':mutated.returncode,'assertion':assertion,'caught':caught,'restored_exit':restored.returncode})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {name}:\n{mutated.stdout}'
 print(f'{name}: 101 {assertion}; restored 0',flush=True)
