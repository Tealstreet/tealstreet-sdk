"""One compiling native control per state slice family; serial cargo, exact restoration."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_ws_state.rs';saved=file.read_bytes();text=saved.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','ws_state','--jobs','2','--offline','--locked']
controls=[
 ('contracts','Some(Self::CommandFailureEnvelopeUndocumented)','Some(Self::CommandRequestIdTypeUndocumented)','WS-STATE native blocker code'),
 ('acknowledgements','time: require_integer(frame.get("time"), &format!("{path}.time"), 0.0)?,','time: require_integer(frame.get("time"), &format!("{path}.time"), 0.0)? + 1.0,','WS-STATE native ack time'),
 ('subscription-state','generation: state.generation + 1.0,','generation: state.generation + 2.0,','WS-STATE native generation'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  assert text.count(old)==1 and old!=new,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-ws-state-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-ws-state-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode,'call_site':'safe-cex-venue-woo/src/v3_ws_state.rs:'+str(text[:text.index(old)].count('\n')+1)})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-ws-state-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'iterations':3,'survivors':0},indent=2)+'\n')
