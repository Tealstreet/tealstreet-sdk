#!/usr/bin/env python3
"""One grouped production control for five public/private ping method families."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=[kind+'.'+name for kind in ['public','private'] for name in ['onOpen','startPingLoop','schedulePing','stopPingLoop','ping']]
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','ws_ping_lifecycle','--jobs','2']
def replace(s,old,new,count=1):
 assert s.count(old)==count,(old,s.count(old));return s.replace(old,new)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/ws_ping_lifecycle.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-ws-ping-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=replace(s,'method(h, socket, "stopPingLoop", &[])?;','method(h, socket, "ping", &[])?;')
  s=replace(s,'let interval = h.get(&class, "PING_INTERVAL_MS")?;','let _interval = h.get(&class, "PING_INTERVAL_MS")?;\n    let interval = h.undefined();')
  s=replace(s,'let id = h.get(socket, "hlPingTimeoutId")?;','let id = h.get(socket, "isConnected")?;')
  s=replace(s,'let value = h.literal(&text("ping"));','let value = h.literal(&text("pong"));')
  s=replace(s,'method(h, socket, "trackLastReceived", &[])?;','method(h, socket, "ping", &[])?;',2)
  source.write_text(s);mutation=run('grouped-ten-mutation')
 finally:source.write_bytes(original)
 restored=run('restored');observed=[n for n in NAMES if re.search(r'failed: PING native[^\n]*'+re.escape(n)+r'\b',mutation.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'logs':str(logs),'iteration_stop':'five fixed method families across ten public/private source roots in one grouped production control; callback guards are positive corpus cases, not a separate executed mutation'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all ten native assertions discriminate; restored test passed',flush=True)
