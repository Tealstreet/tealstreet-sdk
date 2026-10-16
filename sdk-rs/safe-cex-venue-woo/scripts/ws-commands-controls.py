"""One compiling mutation per command slice family; serial cargo, exact restoration."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_ws_commands.rs';saved=file.read_bytes()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','ws_commands','--jobs','2','--offline','--locked']
controls=[
 ('helpers','value.is_some_and(Value::is_object)','value.is_some_and(Value::is_array)','WS-COMMAND native record predicate'),
 ('models','"SUBSCRIBE" => Some(Self::Subscribe)','"SUBSCRIBE" => Some(Self::Unsubscribe)','WS-COMMAND native literal'),
 ('commands','build_command(channel, id, WooV3WsCommandName::Unsubscribe, Some(topics))','build_command(channel, id, WooV3WsCommandName::Subscribe, Some(topics))','WS-COMMAND native command kind'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  text=saved.decode();assert text.count(old)==1,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-ws-commands-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-ws-commands-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-ws-commands-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'iterations':3,'survivors':0},indent=2)+'\n')
