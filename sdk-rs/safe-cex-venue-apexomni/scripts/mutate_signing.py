"""One builder-argument control per signing body, with decoded native operation checks."""
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[3]
p=root/'sdk-rs/safe-cex-venue-apexomni/src/transaction_signing.rs';original=p.read_text();mutant=original
controls=[('pair_id','slot_id','contract_native','signContract operations'),('base_token_id','quote_token_id','order_native','signOrder operations'),('from_sub_account_id','to_sub_account_id','transfer_native','signTransfer operations'),('to_chain_id','l1_target_token_id','withdraw_native','signWithdraw operations'),('chain_id','account_id','change_pubkey_native','signChangePubKey operations')]
for source,target,_,_ in controls:
 before=f'            J::Number(payload.{source}),';after=f'            J::Number(payload.{target}),'
 assert mutant.count(before)==1,before;mutant=mutant.replace(before,after)
cmd=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','signing','--jobs','2','--','--test-threads=1']
logs=root/'sdk-rs/target/apexomni-controls'
try:
 p.write_text(mutant);result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr;(logs/'signing-mutation.log').write_text(output)
 assert result.returncode==101 and 'test result: FAILED.' in output,output
 for _,_,test,label in controls:assert f'test {test} ... FAILED' in output and label in output,(test,label,output)
 print('5 compiled signing controls: exit 101; named native builder assertions failed',flush=True)
finally:p.write_text(original)
result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);(logs/'signing-restored.log').write_text(result.stdout+result.stderr);assert result.returncode==0,result.stdout+result.stderr
print('signing restored: exit 0',flush=True)
