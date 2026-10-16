"""Eight new row faults plus ownership, ready-await, literal-definition and resolution controls."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/private_session.rs';logs=Path(os.environ['MEXC_PRIVATE_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],text=True).strip(),'commit inputs first'
original=source.read_text()
def span(function):
 match=re.search(r'\n(?:pub )?(?:async )?fn '+re.escape(function)+r'\b',original);assert match,function
 begin=match.start();brace=original.index('{',match.end());end=brace+1;depth=1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return begin,end
begin,end=span('start');block=original[begin:end];a=block.index('host.start_task(');b=block.index('}));',a)+4;owner_old=block[a:b]
a=block.index('let value = match body.await');brace=block.index('{',a);depth=1;b=brace+1
while depth:
 if block[b]=='{':depth+=1
 if block[b]=='}':depth-=1
 b+=1
assert block[b]==';';resolution_old=block[a:b+1]
controls=[
 ('connectAndSubscribe','connect_native','connect','if host.disposed() || host.truthy(&host.public_only()?) {','if host.disposed() || { let _ = host.public_only()?; false } {','native private prefix count connectAndSubscribe/2'),
 ('onOpen','open_native','on_open','host.private_snapshot(10)?;','let _ = host;','native private prefix count onOpen/0'),
 ('sendLogin','login_native','send_login','h.send_login_frame(send, data, true)?;','h.send_login_frame(send, data, false)?;','native boolean sendLogin/0/trace/event5/redactAuth'),
 ('createLoginSignature','signature_native','create_login_signature','request_type: "ws-auth"','request_type: "auth"','native word createLoginSignature/1/prefix/event4/type'),
 ('sendWithDiagnostics','send_diagnostics_native','send_diagnostics','host.send(data)?;','let _ = data;','native private prefix count sendWithDiagnostics/0'),
 ('emitReceiveDiagnostics','receive_diagnostics_native','receive_diagnostics','h.global_string(&message)?','h.abstract_string(&message)?','native private formatter success emitReceiveDiagnostics/0/8'),
 ('redactAuthFrame','redact_native','redact_auth_frame','host.define_field(&param, "signature", host.text(units("[redacted]")))?;','let _ = &param;','native UTF-16 redactAuthFrame/0/prefix/event4/value.signature'),
 ('throttledFetchBalance','balance_native','throttled_fetch_balance','host.set_timeout(1000, callback)?','host.set_timeout(999, callback)?','native integer throttledFetchBalance/0/prefix/event2/period'),
 ('boundary.taskOwnership','abandoned_methods_continue','start',owner_old,'let _ = (body, output, resolver);','native private prefix count sendLogin/10'),
 ('boundary.readyAwait','already_ready_children_still_yield','observe','yielded: false','yielded: true','native private prefix count sendLogin/9'),
 ('boundary.literalApiKey','redact_native','redact_auth_frame','host.define_field(&param, "apiKey",','host.set_field(&param, "apiKey",','native private prefix count redactAuthFrame/12'),
 ('boundary.literalSignature','redact_native','redact_auth_frame','host.define_field(&param, "signature",','host.set_field(&param, "signature",','native private prefix count redactAuthFrame/12'),
 ('boundary.returnResolution','signature_native','start',resolution_old,'let value = body.await; let _ = resolver;','native private total count createLoginSignature/12'),
]
assert len(controls)==13 and controls[-1][0]=='boundary.returnResolution'
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':13,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--test','private_session','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '10 passed; 0 failed' in baseline.stdout,baseline.stdout
for method,test,function,old,new,assertion in controls:
 begin,end=span(function);body=original[begin:end];assert body.count(old)==1,method
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:]);broken=run(method+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:source.write_text(original)
 restored=run(method+'-restored',test);summary['controls'].append({'method':method,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {method}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {function}: {broken.stdout}'
 print(f'{method}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
