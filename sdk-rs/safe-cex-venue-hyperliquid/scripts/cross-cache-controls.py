#!/usr/bin/env python3
"""Ten named production controls in one run, always restore the committed bytes."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['getSessionStorage','broadcastCacheUpdate','getPrePlacementKey','readPrePlacementCache','writePrePlacementCache','readPendingOrdersCache','writePendingOrdersCache','hasSeenFill','markFillSeen','dispose']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','cross_cache_leaves','--jobs','2']
def replace_once(text,old,new):
 assert text.count(old)==1,(old,text.count(old));return text.replace(old,new)
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--output',required=True,type=Path);args=parser.parse_args()
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/cross_cache.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-cache-controls-'))
 def run(label):
  result=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(result.stdout);print(label+':exit'+str(result.returncode),flush=True);return result
 try:
  text=original.decode()
  text=replace_once(text,'result.unwrap_or_else(|_| h.null())','{ let _original = result.unwrap_or_else(|_| h.null()); h.undefined() }')
  text=replace_once(text,'let action = h.literal(action);','let action = { let _ = action; h.literal(&text("CONTROL_ACTION")) };')
  text=replace_once(text,".chain([b'|' as u16])",".chain([b'!' as u16])")
  start=text.index('fn read_cache');end=text.index('pub fn read_pre_placement_cache',start)
  body=text[start:end];body=replace_once(body,'Ok(v) => Ok(v)','Ok(_) => Ok(h.undefined())');text=text[:start]+body+text[end:]
  for key in ['tealstreet_hip3_prePlacement','tealstreet_hip3_pendingOrders']:
   pattern=r'(write_cache\(\s*h,\s*cache,\s*value,\s*)"'+key+'"';text,n=re.subn(pattern,r'\1"CONTROL_STORAGE_KEY"',text);assert n==1,(key,n)
  start=text.index('pub fn has_seen_fill');end=text.index('pub fn mark_fill_seen',start)
  body=text[start:end];body=replace_once(body,'let id = h.literal(fill_id);','let id = { let _ = fill_id; h.literal(&text("CONTROL_FILL")) };');text=text[:start]+body+text[end:]
  text=replace_once(text,'h.literal(&text("seenFill"))','h.literal(&text("CONTROL_SEEN"))')
  text=replace_once(text,'let null = h.null();','let null = h.undefined();')
  source.write_text(text);mutated=run('grouped-ten-mutation')
 finally:source.write_bytes(original)
 restored=run('restored')
 observed=[name for name in NAMES if re.search(r'failed: CACHE native[^\n]*\b'+name+r'\b',mutated.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'logs':str(logs),'mutation_exit':mutated.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'iteration_stop':'ten fixed method families, one grouped mutation; not exhaustive historical enumeration'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutated.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutated.stdout and 'error[E' not in mutated.stdout,mutated.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('ten native families discriminate; restored test passed',flush=True)
