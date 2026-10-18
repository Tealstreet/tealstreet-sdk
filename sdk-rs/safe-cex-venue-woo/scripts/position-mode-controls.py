"""Position-mode validation-order control with exact source restoration."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];mode=crate/'src/v3_position_mode.rs';saved={mode:mode.read_bytes()};sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();prefix='/tmp/woo-mode-'+sha[:10]+'-'
text=saved[mode].decode()
def end_block(s,start):
 start_brace=s.index('{',start);depth=0
 for i in range(start_brace,len(s)):
  depth += int(s[i]=='{')-int(s[i]=='}')
  if depth==0:return i+1
 raise AssertionError('unclosed block')
a=text.index('    if account.account_mode != "MARGIN"');b=text.index('    if account.position_mode == target',a);end=end_block(text,b);old_order=text[a:end];new_order=text[b:end]+'\n'+text[a:b]
controls=[(mode,old_order,new_order,'position-mode-order','native_position_mode_builders','MODE native first blocker')]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','position_mode','--jobs','1','--offline','--locked'];rows=[];receipt={'sha':sha,'mode':'one validation-order control; exact assertion failure and restored pass','iterations':0,'survivors':0,'controls':rows}
def record():Path(prefix+'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
for p,old,new,family,test,assertion in controls:
 s=saved[p].decode();assert s.count(old)==1 and old!=new
 rows.append({'family':family,'file':str(p),'old':old,'new':new,'test':test,'assertion':assertion,'overlay_on_sha':sha,'call_site':str(p)+':'+str(s[:s.index(old)].count('\n')+1),'command':command+[test,'--','--exact']})
record()
try:
 for p,old,new,*_ in controls:p.write_text(saved[p].decode().replace(old,new))
 for r in rows:
  run=subprocess.run(r['command'],cwd=crate.parent,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);r['exit']=run.returncode;r['log']=prefix+r['family']+'.log';Path(r['log']).write_text(run.stdout);r['named_failure']=run.returncode==101 and 'panicked at' in run.stdout and r['assertion'] in run.stdout and '0 passed; 1 failed;' in run.stdout
  if not r['named_failure']:r['finding']='survived'if run.returncode==0 else'no named assertion';receipt['survivors']+=int(run.returncode==0)
  record();print(r['family']+' control exit'+str(run.returncode)+' named='+str(r['named_failure']),flush=True)
finally:
 for p,contents in saved.items():p.write_bytes(contents)
assert all(p.read_bytes()==v for p,v in saved.items())
for r in rows:
 run=subprocess.run(r['command'],cwd=crate.parent,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);r['restored_exit']=run.returncode;r['restored_log']=prefix+r['family']+'-restored.log';Path(r['restored_log']).write_text(run.stdout);receipt['iterations']+=1;record();assert run.returncode==0 and '1 passed; 0 failed;'in run.stdout,run.stdout;print(r['family']+' restored exit0',flush=True)
assert all(r['named_failure']for r in rows),receipt
