"""One compiling control per session slice family; native assertions, serial cargo."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_ws_session.rs';saved=file.read_bytes()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','ws_session','--jobs','2','--offline','--locked']
controls=[
 ('scalar-validators','            Ok(n)','            Ok(n + 1.0)','WS-SESSION native integer'),
 ('native-declarations','#[serde(rename = "expiredTime")]\n    expired_time: f64,\n    #[serde(rename = "responseTimestamp")]\n    response_timestamp: f64,','#[serde(rename = "responseTimestamp")]\n    expired_time: f64,\n    #[serde(rename = "expiredTime")]\n    response_timestamp: f64,','WS-SESSION native model expiry'),
 ('listen-session','        expired_time,\n        response_timestamp,','        expired_time: response_timestamp,\n        response_timestamp,','WS-SESSION native listen expiry'),
 ('heartbeat','cmd: "PING".into(),','cmd: "PONG".into(),','WS-SESSION native ping cmd'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  text=saved.decode();assert text.count(old)==1,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-ws-session-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-ws-session-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-ws-session-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'iterations':4,'survivors':0},indent=2)+'\n')
