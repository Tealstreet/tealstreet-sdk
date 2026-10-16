#!/usr/bin/env python3
"""One grouped production mutation for ten new lifecycle/delegation roots."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['core.listenOHLCV','core.listenOrderBook','spot.listenOHLCV','spot.listenOrderBook','core.dispose','spot.dispose','public.dispose','private.dispose','core.endRetryChain','core.scheduleOrderRefresh']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','lifecycle_delegates','--jobs','2']
def replace(s,old,new,count=1):
 assert s.count(old)==count,(old,s.count(old));return s.replace(old,new)
def section(s,start,end,old,new):
 a=s.index(start);b=s.index(end,a);return s[:a]+replace(s[a:b],old,new)+s[b:]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/lifecycle_delegates.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-lifecycle-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=replace(s,'&[first.clone(), callback.clone()]','&[callback.clone(), first.clone()]')
  s=section(s,'pub fn core_dispose','pub fn spot_dispose','let null = h.null();','let null = h.undefined();')
  s=replace(s,'h.super_get(receiver, "dispose")?','h.super_get(receiver, "CONTROL_DISPOSE")?')
  s=replace(s,'method(h, socket, "stopPingLoop", &[])?;','method(h, socket, "CONTROL_STOP", &[])?;',2)
  s=section(s,'pub fn end_retry_chain','pub fn schedule_order_refresh','let no = h.boolean(false);','let no = h.boolean(true);')
  s=section(s,'pub fn run_order_refresh_timeout','pub fn run_order_refresh_error','let no = h.boolean(false);','let no = h.boolean(true);')
  source.write_text(s);mutation=run('grouped-ten-mutation')
 finally:source.write_bytes(original)
 restored=run('restored');observed=[n for n in NAMES if re.search(r'failed: LIFECYCLE native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',mutation.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'logs':str(logs),'pairs':{'delegates':'options/symbol versus distinct callback native handles (and91.25number)','core_dispose':['null','undefined'],'spot_super':['dispose','CONTROL_DISPOSE'],'public_private_ping':['stopPingLoop','CONTROL_STOP'],'end_retry_chain':[False,True],'owned_timeout_pending':[False,True]},'iteration_stop':'tenroots/onegroupedproductioncontrol,111actualsourcecases through110. Ownedtimeoutpendingmutation hits callbacknativeassignment,not onlyregistration. Fullclasses/hostintrinsics/virtualcallees/live/exhaustivebehavior outsideproof.'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all ten named native assertions discriminate; restored test passed',flush=True)
