#!/usr/bin/env python3
"""Ten grouped production controls; each must hit a direct named native assertion."""
import argparse, hashlib, json, re, subprocess, tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['applyCoreClearinghouseState','handleHip3ClearinghouseState','handleFills','applyOptimisticCorePositionFromFill','handleOrderUpdates','applyRealizedPnlToCurrentPosition','generateFakePositionsFromMarkets','syncBalanceFromLivePositions','fetchHip3Positions','parseOpenOrders']
COMMAND=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','core_position_order_state','--jobs','2']
def direct(output,name):
 return bool(re.search(r'(?:failed: DISPATCH native[^\n]*|DISPATCH native (?:number|type|outcome|operation exhausted)[^\n]*)(?<![A-Za-z0-9_])core\.'+re.escape(name)+r'\b',output))
def replace_in_function(s,name,old,new):
 a=s.index('fn '+name+'<');b=s.find('\n/// Source:',a);b=len(s) if b<0 else b
 segment=s[a:b];assert segment.count(old)==1,(name,old,segment.count(old));return s[:a]+segment.replace(old,new)+s[b:]
def replace_first_in_callback(s,variant,old,new):
 a=s.index('PositionCallback::'+variant+'(');b=s.find('\n        PositionCallback::',a+1);assert b>0
 segment=s[a:b];assert old in segment,(variant,old);return s[:a]+segment.replace(old,new,1)+s[b:]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/core_position_order_state.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-position-controls-'))
 def run(label):
  result=subprocess.run(COMMAND,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(result.stdout);print(label+':exit'+str(result.returncode),flush=True);return result
 try:
  s=original.decode()
  s=replace_in_function(s,'apply_core_clearinghouse_state','h.set(&object, "lastCoreClearinghouseStateServerTime", &value)?;','h.set(&object, "lastCoreClearinghouseStateAt", &value)?;')
  # Flip the nullish guard while keeping the break label used (no lint exemptions).
  # A core market with absent hip3Dex must still be the direct regression witness.
  s=replace_in_function(s,'handle_hip3_clearinghouse_state',"if h.is_nullish(&receiver) {\n                            break 'optional_call h.undefined();\n                        }","if !h.is_nullish(&receiver) {\n                            break 'optional_call h.undefined();\n                        }")
  s=replace_first_in_callback(s,'HandleFills0','h.get(&object, "time")?','h.get(&object, "tid")?')
  s=replace_in_function(s,'apply_optimistic_core_position_from_fill','let mut next_entry_price = h.number(0.0);','let mut next_entry_price = h.number(91.25);')
  s=replace_in_function(s,'handle_order_updates','let status = {\n                    let object = update.clone();\n                    h.get(&object, "status")?\n                };','let status = {\n                    let object = update.clone();\n                    h.get(&object, "CONTROL_STATUS")?\n                };')
  s=replace_in_function(s,'apply_realized_pnl_to_current_position','h.define_data(&object, "realizedPnl", &value)?;','h.define_data(&object, "contracts", &value)?;')
  s=replace_first_in_callback(s,'GenerateFakePositionsFromMarkets0','h.define_data(&object, "entryPrice", &value)?;','h.define_data(&object, "contracts", &value)?;')
  s=replace_in_function(s,'sync_balance_from_live_positions','let mut next_upnl = h.number(0.0);','let mut next_upnl = h.number(91.25);')
  s=replace_in_function(s,'fetch_hip3_positions','h.define_data(&object, "dex", &value)?;','h.define_data(&object, "user", &value)?;')
  s=replace_first_in_callback(s,'ParseOpenOrders1','h.get(&object, "origSz")?','h.get(&object, "sz")?')
  source.write_text(s);mutation=run('ten-row-production-controls')
 finally:source.write_bytes(original)
 restored=run('restored')
 observed=[n for n in NAMES if direct(mutation.stdout,n)]
 report={'actual_measured_sha':sha,'command':COMMAND,'mutation_exit':mutation.returncode,'mutation_output':mutation.stdout,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'source_sha256':hashlib.sha256(original).hexdigest(),'scope':'ten grouped controls across ten new rows, including flipped optional dex method guard regression;106case floor through105/9manualawaits; not exhaustive','logs':str(logs)}
 a.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('ten direct named native failures; exact bytes restored; native pass',flush=True)
