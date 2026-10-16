"""One compiling native control per orderbook recovery slice family; serial cargo, exact restoration."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_orderbook_recovery.rs';saved=file.read_bytes();text=saved.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','orderbook_recovery','--jobs','2','--offline','--locked']
start=text.index('    fn create_publication(')
end=text.index('    // ts: recover',start)
publication=text[start:end]
assert publication.count('self.snapshot_timestamp.expect("snapshot time")')==1
controls=[
 ('numeric-level','numeric: [p, q],','numeric: [q, p],','RECOVERY native level price'),
 ('normalization','asks: asks.numeric,','asks: bids.numeric.clone(),','RECOVERY native price'),
 ('request-token','self.next_snapshot_request_token += 1.0;','self.next_snapshot_request_token += 2.0;','RECOVERY native request token'),
 ('buffer','buffered_updates: self.buffered_updates.len(),','buffered_updates: self.buffered_updates.len() + 1,','RECOVERY native buffered count'),
 ('sequence-join','previous_sequence: OrderBookSequence::Number(update.previous_timestamp),','previous_sequence: OrderBookSequence::Number(update.timestamp),','RECOVERY native result variant joined'),
 ('publication',publication,publication.replace('self.snapshot_timestamp.expect("snapshot time")','self.current_timestamp.expect("current time")'),'RECOVERY native snapshot provenance'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  assert text.count(old)==1 and old!=new,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-recovery-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-recovery-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode,'call_site':'safe-cex-venue-woo/src/v3_orderbook_recovery.rs:'+str(text[:text.index(old)].count('\n')+1)})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-recovery-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'iterations':6,'survivors':0},indent=2)+'\n')
