#!/usr/bin/env python3
"""Controls for ten new account roots and the captured-store correction."""
import argparse,json,re,subprocess,tempfile,hashlib
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['core.fetchHistoricalPositions','core.handleSpotClearinghouseStateUpdate','core.validateAccount','core.fetchBuilderFeeApproval','core.fetchCorePerpsState','core.warnOnMismatchedApiWallet','core.fetchAndSetPrivateSnapshot','core.fetchFee','core.fetchPositions','core.fetchAllHip3Positions']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','core_account_snapshots','--jobs','2']
def replace(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
def direct(output,name):
 return bool(re.search(r'(?:failed: DISPATCH native[^\n]*|DISPATCH native (?:number|type|outcome|operation exhausted)[^\n]*)(?<![A-Za-z0-9_])'+re.escape(name)+r'\b',output))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip();subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/core_account_snapshots.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-snapshot-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode();s=replace(s,'let yes = h.boolean(true);\n    h.define_data(&scope, "symbolScoped", &yes)?;','let yes = h.boolean(false);\n    h.define_data(&scope, "symbolScoped", &yes)?;')
  s=replace(s,'let yes = h.boolean(true);\n    h.set(e, "hasSpotCollateralSnapshot", &yes)?;','let yes = h.boolean(false);\n    h.set(e, "hasSpotCollateralSnapshot", &yes)?;')
  s=replace(s,'define_text(h, &result, "error", "")?;','define_text(h, &result, "error", "CONTROL_VALIDATION")?;')
  s=replace(s,'h.set(e, "_builderFeeApproved", &approved)?;\n        h.set(e, "_builderFeeLastCheck", &result)?;','h.set(e, "_builderFeeApproved", &approved)?;\n        h.set(e, "_builderFeeApproved", &result)?;')
  s=replace(s,'h.get(&info, "PERPS_CLEARINGHOUSE_STATE")?','h.get(&info, "USER_FILLS")?')
  s=replace(s,'"Orders will be rejected until the account is re-added."','"CONTROL_WALLET"')
  s=replace(s,'let [orders, core] = h.pair(&result)?;','let [core, orders] = h.pair(&result)?;')
  s=replace(s,'h.define_data(&result, "takerFee", &rates[0])?;','h.define_data(&result, "takerFee", &rates[1])?;')
  s=replace(s,'h.get(entry, "symbol")?','h.get(entry, "contracts")?')
  s=replace(s,'method(h, e, "fetchHip3Positions", &[dex])?','method(h, e, "CONTROL_HIP3", &[dex])?')
  source.write_text(s);grouped=run('grouped-ten-account-roots')
  s=replace(original.decode(),'h.call(&update, &store, &[patch])?;','h.call(&update, &loaded_store, &[patch])?;');source.write_text(s);receiver=run('fresh-store-receiver-regression')
 finally:source.write_bytes(original)
 restored=run('restored');observed=[n for n in NAMES if direct(grouped.stdout,n)];witness='core.fetchAndSetPrivateSnapshot fresh store receiver'
 report={'actual_measured_sha':sha,'command':CARGO,'mutation_exit':grouped.returncode,'mutation_output':grouped.stdout,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'receiver_regression_exit':receiver.returncode,'receiver_regression_output':receiver.stdout,'receiver_required_named_assertion':witness,'receiver_observed':direct(receiver.stdout,witness),'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'source_sha256':hashlib.sha256(original).hexdigest(),'logs':str(logs),'scope':'ten grouped production anchor mutations/newrowfamilies plus one isolated capturedstore receiver regression; fixed100sourcecases through99/50manualawaitpoints; no exhaustive claim'}
 a.output.write_text(json.dumps(report,indent=2)+'\n')
 for r in [grouped,receiver]:assert r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in r.stdout and 'error[E' not in r.stdout,r.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed));assert report['receiver_observed'],'receiver regression survivor atfresh-storecase'
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('ten directnamed failures and isolatedfresh-store failure; restored nativepass',flush=True)
