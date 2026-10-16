#!/usr/bin/env python3
"""One grouped production control for four connection/opening/ping/dispatch method families."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['public.connectAndSubscribe','private.connectAndSubscribe','private.subscribe','spot_public.connectAndSubscribe','spot_public.onOpen','spot_public.ping','spot_private.connectAndSubscribe','spot_private.onOpen','spot_private.ping','spot_private.subscribe']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','ws_transport_leaves','--jobs','2']
def replace(s,old,new,count=1):
 assert s.count(old)==count,(old,s.count(old));return s.replace(old,new)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/ws_transport_leaves.rs';original=source.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-ws-transport-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode()
  s=replace(s,'let urls = h.urls_binding(venue)?;','let urls = h.urls_binding(match venue { WsVenue::Perp => WsVenue::Spot, WsVenue::Spot => WsVenue::Perp })?;')
  s=replace(s,'method(h, socket, "trackLastReceived", &[])?;','method(h, socket, "ping", &[])?;',2)
  s=replace(s,'let ping = h.literal(&text("ping"));','let ping = h.literal(&text("pong"));')
  s=replace(s,'method(h, socket, "subscribeAllActiveAssetData", &[])?;','method(h, socket, "CONTROL_ALL_ASSET_DATA", &[])?;')
  s=replace(s,'method(h, socket, "subscribeCoins", &[])?;','method(h, socket, "CONTROL_COINS", &[])?;')
  source.write_text(s);mutation=run('grouped-ten-mutation')
 finally:source.write_bytes(original)
 restored=run('restored');observed=[n for n in NAMES if re.search(r'failed: TRANSPORT native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',mutation.stdout)]
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original,'logs':str(logs),'iteration_stop':'four fixed connection/opening/ping/dispatch families across ten source roots in one grouped production control; no full class/browser primitive or exhaustive claim'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all ten native assertions discriminate; restored test passed',flush=True)
