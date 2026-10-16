#!/usr/bin/env python3
"""One grouped production control for eleven native helper assertions; restore."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['getFillPositionSide','safeParseNumber','trackProcessedRealizedPnlFill','getRealizedPnlKey','resolveSymbolFromCoin','setRealizedPnlForPosition','incrementRealizedPnlForPosition','getRealizedPnlForPosition','resolvePositionMode','extractCoreBalanceSummary','getMessageTimestamp']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','private_position_helpers','--jobs','2']
def once(s,old,new):
 assert s.count(old)==1,(old,s.count(old));return s.replace(old,new)
def section(s,start,end,old,new):
 a=s.index(start);b=s.index(end,a);return s[:a]+once(s[a:b],old,new)+s[b:]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/private_position_helpers.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-private-position-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=once(s,'h.literal(&text("long"))','h.literal(&text("short"))')
  s=once(s,'{ parsed } else','{ parsed + 1.0 } else')
  s=once(s,'method(h, &ids, "add", &[fill_id.clone()])?','method(h, &ids, "delete", &[fill_id.clone()])?')
  s=once(s,".chain([b':' as u16])",".chain([b'|' as u16])")
  s=section(s,'pub fn resolve_symbol_from_coin','pub fn set_realized_pnl_for_position','h.index(markets_by_id, &coin_value)?','{ let key=h.literal(&text("CONTROL_COIN"));h.index(markets_by_id, &key)? }')
  s=once(s,'h.call(&function, &map, &[key, value.clone()])?','{ let _=value;h.call(&function, &map, &[key, side.clone()])? }')
  s=once(s,'h.add(&old, delta)?','{ let _=old;let zero=h.number_value(0.0);h.add(&zero,delta)? }')
  s=section(s,'pub fn get_realized_pnl_for_position','pub fn resolve_position_mode','h.get(&map, "get")?','h.get(&map, "set")?')
  s=section(s,'pub fn resolve_position_mode','#[derive','"hedged"\n    } else','"CONTROL_MODE"\n    } else')
  s=once(s,'account_value: account,','account_value: account + 1.0,')
  s=once(s,'(timestamp.is_finite() && timestamp > 0.0).then_some(timestamp)','(timestamp.is_finite() && timestamp > 0.0).then_some(timestamp + 1.0)')
  source.write_text(s);mutation=run('grouped-eleven-mutation')
 finally:source.write_bytes(original)
 restored=run('restored');observed=[n for n in NAMES if re.search(r'failed: (?:CACHE|PRIVATE) native[^\n]*'+re.escape(n)+r'\b',mutation.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'logs':str(logs),'iteration_stop':'eleven fixed helper families in one grouped production control; not exhaustive historical coverage'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all eleven native assertions discriminate; restored test passed',flush=True)
