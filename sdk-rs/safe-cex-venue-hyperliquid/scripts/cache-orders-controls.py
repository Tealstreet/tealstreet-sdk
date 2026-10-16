#!/usr/bin/env python3
"""One grouped production control, ten named source methods; restore every byte."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['clearStaleSessionStorage','clearPrePlacement','getPendingOrder','addPendingOrder','removePendingOrder','exchange.addToPrePlacementCache','exchange.matchPrePlacement','exchange.getPendingHip3Order','exchange.addPendingHip3Order','exchange.removePendingHip3Order']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','cross_cache_leaves','--jobs','2']
def once(text,old,new):
 assert text.count(old)==1,(old,text.count(old));return text.replace(old,new)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args()
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/cache_orders.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-cache-orders-controls-'))
 def run(label):
  result=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(result.stdout);print(label+':exit'+str(result.returncode),flush=True);return result
 try:
  text=original.decode()
  text=once(text,'"tealstreet_hip3_seenFills"','"CONTROL_SEEN_STORAGE"')
  text=once(text,'h.literal(&text("removePrePlacement"))','h.literal(&text("CONTROL_PRE"))')
  text=once(text,'Ok(entry)','{ let _ = entry; Ok(h.undefined()) }')
  text=once(text,'h.define_data(&entry, &text("price"), price)?;','{ let _ = price; h.define_data(&entry, &text("price"), kind)?; }')
  text=once(text,'h.literal(&text("removePending"))','h.literal(&text("CONTROL_PENDING"))')
  text=once(text,'method(h, &cache, name, args)','{ let _ = args; method(h, &cache, name, &[]) }')
  source.write_text(text);mutation=run('grouped-ten-mutation')
 finally:source.write_bytes(original)
 restored=run('restored')
 observed=[n for n in NAMES if re.search(r'failed: (?:CACHE|ORDERS) native[^\n]*'+re.escape(n)+r'\b',mutation.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'logs':str(logs),'iteration_stop':'ten source method families in one grouped production control; existing leaf test remains positive; no exhaustive historical claim'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 1 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '2 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all ten named native assertions discriminate; restored two tests passed',flush=True)
