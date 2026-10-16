"""One isolated compiled production fault per newly ported socket method."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/ws_lifecycle.rs';logs=Path(os.environ['POLYMARKET_LIFECYCLE_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(),'commit gate inputs first'
controls=[
 ('public.onOpen','public_open','public_on_open','host.send_subscription()?;','let _ = host;','native socket trace:public.onOpen'),
 ('public.onClose','public_close','public_on_close','host.clear_ping_interval()?;','let _ = host;','native socket trace:public.onClose'),
 ('public.ping','public_ping','ping_public','if host.is_connected() {','if !host.is_connected() {','native socket trace:public.ping'),
 ('public.sendSubscription','public_subscription','send_public_subscription','custom_feature_enabled: Some(true),','custom_feature_enabled: Some(false),','native custom feature:public.sendSubscription'),
 ('private.onOpen','private_open','private_on_open','host.authenticate()?;','let _ = host;','native socket trace:private.onOpen'),
 ('private.onClose','private_close','private_on_close','host.clear_ping_interval()?;','let _ = host;','native socket trace:private.onClose'),
 ('private.ping','private_ping','ping_private','if host.is_connected() {','if !host.is_connected() {','native socket trace:private.ping'),
 ('private.authenticate','private_auth','authenticate_private','channel: message_type("USER"),','channel: message_type("MARKET"),','native subscription channel:private.authenticate'),
]
assert len(controls)==8 and controls[-1][0]=='private.authenticate'
original=source.read_text();summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'floor':'eight new method bodies; stopped after private.authenticate','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','ws_lifecycle','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout);return r
baseline=run('baseline');assert baseline.returncode==0 and '9 passed; 0 failed' in baseline.stdout,baseline.stdout;summary['baseline_exit']=baseline.returncode
for name,test,function,old,new,assertion in controls:
 match=re.search(r'\npub fn '+re.escape(function)+r'\b',original);assert match,name;begin=match.start();following=re.search(r'\npub fn ',original[match.end():]);end=match.end()+following.start() if following else len(original);body=original[begin:end];assert body.count(old)==1,name
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:]);broken=run(name+'-mutation',test);caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'method':name,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure: {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
