#!/usr/bin/env python3
"""New eleven-root control and four observed allocation-order regressions."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['agent.switchToCorrectNetwork','agent.signAgentApproval','agent.submitAgentApproval','agent.createAgentWalletWithBrowserWallet','agent.connectBrowserWallet','builder.switchToCorrectNetwork','builder.checkBuilderFeeApproved','builder.signBuilderFeeApproval','builder.submitBuilderFeeApproval','builder.createBuilderInfo','builder.approveBuilderFeeWithBrowserWallet']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','browser_approval_flows','--jobs','2']
def replace(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
def observed(s,names):return[n for n in names if re.search(r'failed: APPROVAL native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',s)]
def late_object(s,spaces,expression):
 pad=' '*spaces;block=pad+'let denied = h.object()?;\n'+pad+'let no = h.boolean(false);\n'+pad+'h.define_data(&denied, "success", &no)?;\n';return replace(s,block+pad+expression,pad+expression+'\n'+block.rstrip())
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip();subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/browser_approval_flows.rs';test=CRATE/'tests/browser_approval_flows.rs';oracle=CRATE/'scripts/browser-approval-oracle.cjs';original=source.read_bytes();test_bytes=test.read_bytes();oracle_bytes=oracle.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-browser-approval-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode();s=replace(s,'text_field(h, &args, "method", "wallet_switchEthereumChain")?;','text_field(h, &args, "method", "CONTROL_SWITCH")?;');s=replace(s,'h.define_data(&sig, "r", &r)?;','h.define_data(&sig, "r", &s)?;');s=replace(s,'h.define_data(&sig, "s", &s)?;','h.define_data(&sig, "s", &r)?;');s=replace(s,'"approveAgent"','"CONTROL_AGENT"');s=replace(s,'"approveBuilderFee"','"CONTROL_BUILDER"');s=replace(s,'free(h, "switchToCorrectNetwork", &[testnet.clone()])?','free(h, "CONTROL_NETWORK", &[testnet.clone()])?');s=replace(s,'free(h, "hasEthereumWallet", &[])?','free(h, "CONTROL_HAS_WALLET", &[])?');s=replace(s,'h.compare(RequestComparison::GreaterEqual, &value, &required)?','h.compare(RequestComparison::Less, &value, &required)?');s=replace(s,'h.define_data(&o, "b", &lower)?;','h.define_data(&o, "b", &fee)?;');s=replace(s,'h.define_data(&o, "f", &fee)?;','h.define_data(&o, "f", &lower)?;');source.write_text(s);mutation=run('grouped-eleven-production')
 finally:source.write_bytes(original)
 try:
  s=late_object(original.decode(),4,'let name = h.get(&target, "chainName")?;');s=late_object(s,12,'let message = h.get(&parsed, "response")?;');s=late_object(s,12,'let msg = h.get(&network, "error")?;');s=late_object(s,12,'let msg = wrong_wallet(h, expected, &actual)?;');source.write_text(s);regression=run('grouped-four-allocation-order-regressions')
 finally:source.write_bytes(original)
 restored=run('restored');named=observed(mutation.stdout,NAMES)
 # Four independent call-site assertions: two module roots per call site,
 # distinct labels for the two browser branches prevent cross-branch masking.
 sites=[('switch_network final failure',['agent.switchToCorrectNetwork','builder.switchToCorrectNetwork'],'retry'),('submit denial',['agent.submitAgentApproval','builder.submitBuilderFeeApproval'],'submission denial'),('browser_flow network failure',['agent.createAgentWalletWithBrowserWallet','builder.approveBuilderFeeWithBrowserWallet'],'network denial'),('browser_flow mismatch',['agent.createAgentWalletWithBrowserWallet','builder.approveBuilderFeeWithBrowserWallet'],'wallet mismatch')]
 site_results=[]
 for site,roots,label in sites:
  found=[n for n in roots if re.search(r'failed: APPROVAL native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b[^\n]*'+re.escape('"'+label+'"'),regression.stdout)];site_results.append({'call_site':site,'required_roots':roots,'case_label':label,'observed':found,'pair':'source object + false success before dynamic field vs old Rust dynamic getter/call first'})
 report={'measured_sha':sha,'command':CARGO,'production_mutation_exit':mutation.returncode,'required_native_assertions':NAMES,'observed_native_assertions':named,'production_output':mutation.stdout,'allocation_regression_exit':regression.returncode,'allocation_regression_output':regression.stdout,'allocation_call_sites':site_results,'restored_exit':restored.returncode,'restored_output':restored.stdout,'source_bytes_restored':source.read_bytes()==original,'test_bytes_unchanged':test.read_bytes()==test_bytes,'oracle_bytes_unchanged':oracle.read_bytes()==oracle_bytes,'pairs':{'signature_fields':['R-left','S-right'],'factory_b_vs_f':['0xabcdef',17.5],'approval_threshold_operators':['greaterEqual','less'],'switch_method':['wallet_switchEthereumChain','CONTROL_SWITCH'],'submitted_action_types':['approveAgent/approveBuilderFee','CONTROL_AGENT/CONTROL_BUILDER'],'browser_sibling':['switchToCorrectNetwork','CONTROL_NETWORK'],'wallet_guard':['hasEthereumWallet','CONTROL_HAS_WALLET']},'iteration_stop':'11roots;7family production mutations across10replacements;4observedallocation regressions eachwith2module+case named witnesses (8requiredassertions);134sourcecases through133/227manually expectedhostawaits. Firstpoll/hostawait only; no livewallet/network/signing/fullclasses/eagerasync proof.','logs':str(logs)};args.output.write_text(json.dumps(report,indent=2)+'\n')
 for r in [mutation,regression]:assert r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in r.stdout and 'error[E' not in r.stdout and 'could not compile' not in r.stdout,r.stdout
 assert named==NAMES,'survivors:'+repr(set(NAMES)-set(named))
 for s in site_results:assert s['observed']==s['required_roots'],'survivingallocationcallsite:'+repr(s)
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert all(report[k] for k in ['source_bytes_restored','test_bytes_unchanged','oracle_bytes_unchanged']);print('all eleven native assertions and eight allocation case assertions discriminate; restored test passes',flush=True)
