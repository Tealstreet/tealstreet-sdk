#!/usr/bin/env python3
"""Grouped controls for the ten new core routing/ticker roots; no old-row matrix."""
import argparse,json,re,subprocess,tempfile,hashlib
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1]; ROOT=CRATE.parents[1]
NAMES=['core.isStaleMessage','core.onMessage','core.processMessage','core.handleFundings','core.handleUserNonFundingLedgerUpdates','core.handleUserEvents','core.handleServerError','core.handleHip3AllMids','public.handleAllMids','public.handleFastAssetCtxs']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','private_message_dispatch','--jobs','2']
def replace(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);args=p.parse_args()
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 path=CRATE/'src/private_message_dispatch.rs';original=path.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-core-message-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=replace(s,'h.operator(DispatchOperator::LessEqual, &time, &last)?','h.operator(DispatchOperator::Less, &time, &last)?')
  s=replace(s,'method(h, socket, "recordMessageHandlerStats", &[lit(h, "pong")])?;','method(h, socket, "recordMessageHandlerStats", &[lit(h, "CONTROL")])?;')
  s=replace(s,'("allMids", "handleHip3AllMids", 0)','("allMids", "handleServerError", 0)')
  s=replace(s,'_data: &H::Value,\n) -> Result<H::Value, H::Error> {\n    Ok(h.undefined())','_data: &H::Value,\n) -> Result<H::Value, H::Error> {\n    Ok(h.boolean(false))')
  s=replace(s,'method(h, &parent, "requestUnifiedSpotCollateralRefresh", &[])?;','method(h, &parent, "CONTROL_REFRESH", &[])?;')
  s=replace(s,'for name in ["tid", "coin", "px", "sz"]','for name in ["tid", "coin", "fee", "sz"]')
  s=replace(s,'h.get(&severity, "Warning")?','h.get(&severity, "Error")?')
  s=replace(s,'h.set(&buffer, "midPrice", &last)?;','let wrong = h.number(17.5);\n        h.set(&buffer, "midPrice", &wrong)?;')
  s=replace(s,'h.set(&buffer, "mark", &mark)?;','h.set(&buffer, "midPrice", &mark)?;')
  path.write_text(s);mutation=run('grouped-ten-roots')
 finally:path.write_bytes(original)
 restored=run('restored')
 observed=[n for n in NAMES if re.search(r'(?:failed: DISPATCH native[^\n]*|DISPATCH native (?:number|type|outcome|operation exhausted)[^\n]*)(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',mutation.stdout)]
 report={'actual_measured_sha':sha,'command':CARGO,'mutation_exit':mutation.returncode,'mutation_output':mutation.stdout,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':path.read_bytes()==original,'source_sha256':hashlib.sha256(original).hexdigest(),'logs':str(logs),'control_scope':'nine grouped source replacements across ten new core roots; timestamp138<=138, pong-vsCONTROL UTF16, mids/server route identity, undefined-vsfalse no-op return, refresh binding, px-vsfee property presence, Warning-vsError, mid91.25-vs17.5 bits, mark-vsmid field; fixed140case floor through139; no exhaustiveness claim'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('ten named native failures, restored native pass',flush=True)
