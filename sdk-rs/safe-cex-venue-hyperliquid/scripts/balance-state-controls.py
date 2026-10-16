#!/usr/bin/env python3
"""Ten grouped new balance-root controls and the two observed harness regressions."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['core.getSpotCollateralMarkPrice','core.buildUnifiedSpotCollateralSnapshot','core.getAuthoritativeUnifiedUsdcAvailability','core.applyBalanceFromCurrentMode','core.applyCorePerpsBalanceSnapshot','core.requestUnifiedSpotCollateralRefresh','spot.getSpotCoinMarkPrice','spot.buildSpotBalance','spot.applySpotBalances','spot.requestSpotBalanceRefresh']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','balance_state','--jobs','2']
FOROF="""if(ts.isForOfStatement(n)){
   const expression=call('__iterate',[ts.visitNode(n.expression,visit)]),body=ts.visitNode(n.statement,visit);
   if(ts.isVariableDeclarationList(n.initializer)&&n.initializer.declarations.length===1&&ts.isArrayBindingPattern(n.initializer.declarations[0].name)){
    const declaration=n.initializer.declarations[0];assert.equal(declaration.name.elements.length,2,'tuple binding floor');
    const raw=f.createIdentifier('__tupleValue'),initializer=f.createVariableDeclarationList([f.createVariableDeclaration(raw)],ts.NodeFlags.Const);
    const binding=variable(declaration.name,call('__pair',[raw]));
    return f.updateForOfStatement(n,n.awaitModifier,initializer,expression,f.createBlock([binding,...(ts.isBlock(body)?body.statements:[body])],true));
   }
   return f.updateForOfStatement(n,n.awaitModifier,n.initializer,expression,body);
  }"""
def replace(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
def section(s,start,end,old,new):
 a=s.index(start);b=s.index(end,a);return s[:a]+replace(s[a:b],old,new)+s[b:]
def observed(output,names):return[n for n in names if re.search(r'failed: BALANCE native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',output)]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/balance_state.rs';oracle=CRATE/'scripts/balance-state-oracle.cjs';test=CRATE/'tests/balance_state.rs';original=source.read_bytes();original_oracle=oracle.read_bytes();original_test=test.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-balance-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=section(s,'pub fn get_spot_collateral_mark_price','pub fn run_spot_market_predicate','return Ok(h.number(1.0));','return Ok(h.number(2.0));')
  s=section(s,'pub fn run_spot_market_predicate','pub fn get_spot_coin_mark_price','optional_upper(h, candidate, "base")?','optional_upper(h, candidate, "CONTROL_BASE")?')
  s=section(s,'pub fn build_unified_spot_collateral_snapshot','pub fn build_spot_balance','h.define_data(&currency, "used", &hold)?;','h.define_data(&currency, "used", &amount)?;')
  s=section(s,'pub fn build_spot_balance','pub fn get_authoritative_unified_usdc_availability','h.define_data(&currency, "used", &hold)?;','h.define_data(&currency, "used", &amount)?;')
  s=section(s,'pub fn get_authoritative_unified_usdc_availability','fn snapshot','let zero = h.number(0.0);','let zero = h.number(1.0);')
  s=section(s,'pub fn apply_balance_from_current_mode','pub fn apply_core_perps_balance_snapshot','h.define_data(&balance, "used", &used)?;','h.define_data(&balance, "used", &upnl)?;')
  s=section(s,'pub fn apply_core_perps_balance_snapshot','pub fn apply_spot_balances','for name in ["accountValue", "totalMarginUsed", "upnl"]','for name in ["totalMarginUsed", "accountValue", "upnl"]')
  s=section(s,'pub fn apply_spot_balances','fn request','let array = h.global("Array")?;','let array = h.global("CONTROL_ARRAY")?;')
  s=replace(s,'request(h, exchange, force, "refreshUnifiedSpotCollateralBalance")','request(h, exchange, force, "CONTROL_CORE_REFRESH")')
  s=replace(s,'request(h, exchange, force, "refreshSpotBalance")','request(h, exchange, force, "CONTROL_SPOT_REFRESH")')
  source.write_text(s);mutation=run('grouped-ten-production')
 finally:source.write_bytes(original)
 regressions=[]
 try:
  oracle.write_text(replace(original_oracle.decode(),FOROF,"if(ts.isForOfStatement(n))return f.updateForOfStatement(n,n.awaitModifier,n.initializer,call('__iterate',[ts.visitNode(n.expression,visit)]),ts.visitNode(n.statement,visit));"))
  r=run('oracle-uninstrumented-tuple-binding');regressions.append({'call_site':'balance-state-oracle.cjs::transformer ForOf initializer','required':'core.getAuthoritativeUnifiedUsdcAvailability','exit':r.returncode,'output':r.stdout,'observed':observed(r.stdout,['core.getAuthoritativeUnifiedUsdcAvailability']),'pair':'token=0 available=91.25: old oracle global Number vs required native pair operation'})
 finally:oracle.write_bytes(original_oracle)
 try:
  test.write_text(replace(original_test.decode(),'if f.tag() == "find" && frame["findInvoked"] == true {','if f.tag() == "find" {'))
  r=run('adapter-callback-after-find-rejection');regressions.append({'call_site':'tests/balance_state.rs::Host::call find branch','required':'spot.getSpotCoinMarkPrice','exit':r.returncode,'output':r.stdout,'observed':observed(r.stdout,['spot.getSpotCoinMarkPrice']),'pair':'find rejects error handle14 before callback vs invalid adapter runs empty find returning undefined'})
 finally:test.write_bytes(original_test)
 restored=run('restored');named=observed(mutation.stdout,NAMES)
 report={'measured_sha':sha,'command':CARGO,'mutation_exit':mutation.returncode,'required_native_assertions':NAMES,'observed_native_assertions':named,'mutation_output':mutation.stdout,'harness_regressions':regressions,'restored_exit':restored.returncode,'restored_output':restored.stdout,'source_bytes_restored':source.read_bytes()==original,'oracle_bytes_restored':oracle.read_bytes()==original_oracle,'test_bytes_restored':test.read_bytes()==original_test,'pairs':{'stable_price':[1,2],'currency_used':[1.25,4.75],'authoritative_token':[0,1],'perp_field_order':['accountValue','totalMarginUsed'],'standard_used':[91.25,17.5],'owned_spot_predicate_get':['base','CONTROL_BASE'],'spot_array_global':['Array','CONTROL_ARRAY'],'refresh_methods':['refreshUnifiedSpotCollateralBalance/refreshSpotBalance','CONTROL_CORE_REFRESH/CONTROL_SPOT_REFRESH']},'logs':str(logs),'iteration_stop':'tenroots/onegroupedproduction plus two concrete observed harness regressions; source152through151; no universal or full-class claim'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert named==NAMES,'survivors:'+repr(set(NAMES)-set(named))
 for r in regressions:assert r['exit']==101 and r['observed']==[r['required']] and 'test result: FAILED. 0 passed; 1 failed' in r['output'] and 'error[E' not in r['output'],r
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert all(report[k] for k in ['source_bytes_restored','oracle_bytes_restored','test_bytes_restored']);print('ten exact native assertions and both observed harness regressions discriminate; restored test passes',flush=True)
