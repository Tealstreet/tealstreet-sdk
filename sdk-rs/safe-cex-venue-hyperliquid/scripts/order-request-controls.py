#!/usr/bin/env python3
"""New ten-root grouped controls and regressions for observed production/harness gaps."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['core.getBuilderInfo','core.getOrderErrorTelemetry','core.postOrderAction','core.validateResponseAndExtractOrderIds','core.signActionOrDefer','core.scheduleCancel','core.setLeverage','core.changeMarginType','spot.setLeverage','spot.changeMarginType']
VAULT=['core.scheduleCancel','core.setLeverage','core.changeMarginType']
HARNESS=['core.getOrderErrorTelemetry',*VAULT]
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','order_request_leaves','--jobs','2']
def replace(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
def section(s,start,end,old,new):
 a=s.index(start);b=s.index(end,a);return s[:a]+replace(s[a:b],old,new)+s[b:]
def observed(output,names):return[n for n in names if re.search(r'failed: REQUEST native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',output)]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/order_request_leaves.rs';test=CRATE/'tests/order_request_leaves.rs';oracle=CRATE/'scripts/order-request-oracle.cjs';original=source.read_bytes();original_test=test.read_bytes();original_oracle=oracle.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-order-request-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode();s=replace(s,'let fee = h.get(&builder, "feeTenthsBps")?;','let _fee = h.get(&builder, "feeTenthsBps")?;');s=replace(s,'h.define_data(&result, "f", &fee)?;','h.define_data(&result, "f", &lower)?;')
  s=replace(s,'define_text(h, &tags, "hyperliquid_error_family", "builder_fee")?;','define_text(h, &tags, "hyperliquid_error_family", "CONTROL_BUILDER")?;')
  s=section(s,'pub async fn post_order_action','pub fn validate_response_and_extract_order_ids','method(h, e, "scheduleOrderRefresh", &[])?;','method(h, e, "CONTROL_REFRESH", &[])?;')
  s=replace(s,'if h.instance_of(&error, &class)? {','if !h.instance_of(&error, &class)? {')
  s=replace(s,'let mainnet = h.boolean(!h.truthy(&testnet));','let mainnet = h.boolean(h.truthy(&testnet));')
  s=replace(s,'let five = h.number(5000.0);','let five = h.number(5001.0);')
  s=replace(s,'define_text(h, &a, "type", "updateLeverage")?;','define_text(h, &a, "type", "CONTROL_LEVERAGE")?;')
  source.write_text(s);mutation=run('grouped-ten-production')
 finally:source.write_bytes(original)
 try:
  s=section(original.decode(),'fn vault_spread','async fn leverage_request','    let vault = options(h, e, "vault")?;\n    let spread = h.object()?;','    let spread = h.object()?;\n    let vault = options(h, e, "vault")?;')
  source.write_text(s);vault=run('regression-vault-object-before-condition')
 finally:source.write_bytes(original)
 try:
  s=replace(original_test.decode(),'Native::Handle(_, _, kind) => k == kind,','Native::Handle(_, tag, _) => k == if tag == "function" { "function" } else { "object" },')
  s=section(s,'    fn spread_into','    fn spot_market_predicate','        self.operations.set(self.operations.get() + 1);\n','')
  test.write_text(s);harness=run('grouped-two-harness-regressions')
 finally:test.write_bytes(original_test)
 restored=run('restored');named=observed(mutation.stdout,NAMES);vault_named=observed(vault.stdout,VAULT);harness_named=observed(harness.stdout,HARNESS)
 report={'measured_sha':sha,'command':CARGO,'production_mutation_exit':mutation.returncode,'required_native_assertions':NAMES,'observed_native_assertions':named,'production_output':mutation.stdout,'vault_regression':{'exit':vault.returncode,'required':VAULT,'observed':vault_named,'output':vault.stdout,'call_site':'order_request_leaves.rs::vault_spread','pair':'source GetV(options,vault) before selected spread object vs old Rust object allocation first'},'harness_regressions':{'exit':harness.returncode,'required':HARNESS,'observed':harness_named,'output':harness.stdout,'call_sites':['Host::type_is: source function field tagged action/builder remains typeof function','Host::spread_into: every consumed spread frame must increment operationcounter'],'pairs':['function vs object for tagged callable: native get argument distinguishes','await operation ordinal includes both signature and vault spread vs missing increments: named native suspension cutpoints distinguish']},'restored_exit':restored.returncode,'restored_output':restored.stdout,'source_bytes_restored':source.read_bytes()==original,'test_bytes_restored':test.read_bytes()==original_test,'oracle_bytes_unchanged':oracle.read_bytes()==original_oracle,'pairs':{'builder_fee':[17.5,'0xabcdef1234567890'],'refresh_count':[1,0],'sign_mainnet':[True,False],'minimum_deadline_offset':[5000,5001],'action_type':['updateLeverage','CONTROL_LEVERAGE'],'telemetry_family':['builder_fee','CONTROL_BUILDER']},'iteration_stop':'10roots/7semanticproductionmutations/8anchorreplacements plus1observedvault-order regression and2groupedharness-regressions;143sourcecases through142/94awaits. First-poll/host-await body only; eager JS-call scheduling/fullclasses/live/signing outsideproof.','logs':str(logs)}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 for r in [mutation,vault,harness]:assert r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in r.stdout and 'error[E' not in r.stdout,r.stdout
 assert named==NAMES,'survivors:'+repr(set(NAMES)-set(named));assert vault_named==VAULT,vault.stdout;assert harness_named==HARNESS,harness.stdout
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert all(report[k] for k in ['source_bytes_restored','test_bytes_restored','oracle_bytes_unchanged']);print('ten exact native assertions, vault-order and both harness regressions discriminate; restored test passes',flush=True)
