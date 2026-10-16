"""Three serial controls: contracts, literals, topic logic; exact byte restoration."""
import json,os,subprocess
from pathlib import Path
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1]
file=crate/'src/v3_ws_topics.rs';saved=file.read_bytes()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','ws_topics','--jobs','2','--offline','--locked']
controls=[
 ('contracts','maximum_topics_per_command: 20','maximum_topics_per_command: 21','WS-TOPIC native maximum command topics'),
 ('literals','"bbo" => Some(Self::Bbo)','"bbo" => Some(Self::Bborpi)','WS-TOPIC native public enum'),
 ('topics','includes_rpi: kind == WooV3WsPublicTopicKind::Orderbookupdaterpi','includes_rpi: kind == WooV3WsPublicTopicKind::Orderbookupdate','WS-TOPIC native includes RPI'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  source=saved.decode();assert old in source
  # bbo literal occurs in both PublicTopicKind and PlainKind: mutate the former only.
  file.write_text(source.replace(old,new,1))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-ws-topics-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  results.append({'family':name,'exit':run.returncode,'assertion':assertion});print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-ws-topics-restored.log').write_text(restored.stdout);assert restored.returncode==0,restored.stdout
 results[-1]['restored_exit']=restored.returncode
 print(name+' restored exit 0',flush=True)
 
Path('/tmp/woo-ws-topics-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'restored_exit':restored.returncode,'iterations':3,'survivors':0},indent=2)+'\n')
print('Restored exit 0; iteration 3; no survivors in this floor',flush=True)
