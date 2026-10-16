#!/usr/bin/env python3
"""Ten production row controls plus isolated dropped-root cancellation fixture."""
import argparse,hashlib,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['fetchMarketsHelper','fetchHip3Dexs','fetchHip3Markets','fetchOHLCV','fetchIncomeHistory','fetchHistoricalOrders','deriveClosedPositions','fetchIncomeHistoryRange','fetchHistoricalFills','fetchFillsForOrderIds']
COMMAND=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','core_rest_reads','--jobs','2']
def direct(output,name):
 return bool(re.search(r'(?:failed: (?:DISPATCH|REST) native[^\n]*|(?:DISPATCH|REST) native (?:number|type|outcome|operation exhausted|dropped root)[^\n]*)(?<![A-Za-z0-9_])core\.'+re.escape(name)+r'\b',output))
def scoped(s,kind,name,old,new):
 a=s.index(('fn '+name+'<')if kind=='fn'else('RestCallback::'+name+'('));b=s.find('\n/// Source:',a)if kind=='fn'else s.find('\n        RestCallback::',a+1);b=len(s)if b<0 else b
 part=s[a:b];assert part.count(old)==1,(kind,name,old,part.count(old));return s[:a]+part.replace(old,new)+s[b:]
def mutations(s):
 s=scoped(s,'fn','fetch_markets_helper','h.define_data(&object, "quote", &value)?;','h.define_data(&object, "base", &value)?;')
 s=scoped(s,'callback','FetchHip3Dexs0','h.define_data(&object, "dexPosition", &value)?;','h.define_data(&object, "hip3Dex", &value)?;')
 s=scoped(s,'fn','fetch_hip3_markets_body','h.number(110000.0)','h.number(120000.0)')
 s=scoped(s,'callback','FetchOHLCV1','h.number(1000.0)','h.number(100.0)')
 s=scoped(s,'fn','fetch_income_history_body','h.get(&object, "startTime")?','h.get(&object, "endTime")?')
 s=scoped(s,'fn','fetch_historical_orders_body','h.number(2000.0)','h.number(1999.0)')
 s=scoped(s,'fn','derive_closed_positions_body','lit(h, "")','lit(h, "CONTROL_ACCOUNT")')
 s=scoped(s,'fn','fetch_income_history_range_body','h.define_data(&object, "startTime", &value)?;','h.define_data(&object, "endTime", &value)?;')
 s=scoped(s,'fn','fetch_historical_fills_body','h.define_data(&object, "venueReportedRevenue", &value)?;','h.define_data(&object, "commission", &value)?;')
 s=scoped(s,'fn','fetch_fills_for_order_ids_body','let value = h.boolean(true);','let value = h.boolean(false);')
 return s
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip();subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/core_rest_reads.rs';test=CRATE/'tests/core_rest_reads.rs';original=source.read_bytes();original_test=test.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-rest-controls-'))
 def run(label):
  r=subprocess.run(COMMAND,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  source.write_text(mutations(original.decode()));grouped=run('ten-production-roots');source.write_bytes(original)
  s=original_test.decode();anchor='drop(observer);';assert s.count(anchor)==2
  # One drop is the local callback adapter; the indented one is the root-drop witness.
  anchor='                        drop(observer);';assert s.count(anchor)==1
  s=s.replace(anchor,anchor+'\n                        h.r.borrow_mut().tasks.retain(|(owner,_)|*owner!=0);');test.write_text(s);cancel=run('cancel-dropped-root-regression')
 finally:source.write_bytes(original);test.write_bytes(original_test)
 restored=run('restored');observed=[n for n in NAMES if direct(grouped.stdout,n)];drop_names=['fetchHip3Dexs','fetchIncomeHistory','fetchHistoricalFills'];drop_observed=[n for n in drop_names if direct(cancel.stdout,n)]
 report={'actual_measured_sha':sha,'command':COMMAND,'mutation_exit':grouped.returncode,'mutation_output':grouped.stdout,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'cancel_exit':cancel.returncode,'cancel_output':cancel.stdout,'cancel_required_native_roots':drop_names,'cancel_observed_native_roots':drop_observed,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original and test.read_bytes()==original_test,'source_sha256':hashlib.sha256(original).hexdigest(),'test_sha256':hashlib.sha256(original_test).hexdigest(),'logs':str(logs),'scope':'ten grouped production controls/newrows plus one isolated runtimefixture that cancels root task ownership after observer drop;94sourcecases through93,97native replays including3rootdrops through2;101sourceawaits/108replayedawaits, no exhaustive claim'}
 a.output.write_text(json.dumps(report,indent=2)+'\n')
 for r in [grouped,cancel]:assert r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in r.stdout and 'error[E' not in r.stdout,r.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed));assert drop_observed==drop_names,'drop survivors:'+repr(set(drop_names)-set(drop_observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('ten direct native row failures; three direct root-drop failures; restored native pass',flush=True)
