"""One isolated compiled missing-step/guard fault per newly ported method body."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/ws_lifecycle.rs';logs=Path(os.environ['MEXC_LIFECYCLE_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],text=True).strip(),'commit gate inputs first'
controls=[
 ('public.startHeartbeat','public_start','start_public_heartbeat','stop_public_heartbeat(host.as_ref(), slot)?;','let _ = slot;'),
 ('public.stopHeartbeat','public_stop','stop_public_heartbeat','host.clear_interval(handle)?;','let _ = (host, handle);'),
 ('public.handleClose','public_close','handle_public_close','stop()?;','let _ = stop;'),
 ('public.onOpen','public_open','public_on_open','start()?;','let _ = start;'),
 ('public.ping','public_ping','ping_public','if !is_disposed {','if is_disposed {'),
 ('public.dispose','public_dispose','dispose_public','off("activeTickerSymbols:changed", handler())?;','let _ = (off, handler);'),
 ('private.startHeartbeat','private_start','start_private_heartbeat','stop_private_heartbeat(host.as_ref(), slot)?;','let _ = slot;'),
 ('private.stopHeartbeat','private_stop','stop_private_heartbeat','host.clear_interval(handle)?;','let _ = (host, handle);'),
 ('private.handleClose','private_close','handle_private_close','stop()?;','let _ = stop;'),
 ('private.dispose','private_dispose','dispose_private','clear_timeout(handle)?;','let _ = (clear_timeout, handle);'),
]
assert len(controls)==10 and controls[-1][0]=='private.dispose'
original=source.read_text();summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'floor':'ten new method bodies; stopped after private.dispose','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--test','ws_lifecycle','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(label+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout);return r
baseline=run('baseline');assert baseline.returncode==0 and '10 passed; 0 failed' in baseline.stdout,baseline.stdout
summary['baseline_exit']=baseline.returncode
for name,test,function,old,new in controls:
 match=re.search(r'\npub fn '+re.escape(function)+r'\b',original);assert match,name
 begin=match.start();following=re.search(r'\npub fn ',original[match.end():]);end=match.end()+following.start() if following else len(original)
 body=original[begin:end];assert body.count(old)==1,name
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:]);broken=run(name+'-mutation',test)
  assertion='native lifecycle trace:'+name
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test)
 summary['controls'].append({'method':name,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure: {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
