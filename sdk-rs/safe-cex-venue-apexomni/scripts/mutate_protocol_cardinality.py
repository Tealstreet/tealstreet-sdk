"""Protocol collection cardinality controls, batched per complete live TS corpus."""
from pathlib import Path
import os
import re
import subprocess
ROOT=Path(__file__).resolve().parents[3]
P=ROOT/'sdk-rs/safe-cex-venue-apexomni'
LOGS=ROOT/'sdk-rs/target/apexomni-controls'
BASE=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--locked','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--jobs','1','--test','protocol','protocol_matches_live_typescript','--','--nocapture']
def run(label,code,names):
 r=subprocess.run(BASE,cwd=ROOT,capture_output=True,text=True);out=r.stdout+r.stderr
 (LOGS/(label+'.log')).write_text(out)
 assert r.returncode==code,(label,r.returncode,out)
 for name in names:assert name in out,(label,name,out)
 if code==101:assert 'test result: FAILED.' in out
 print(label,r.returncode,len(names),'named assertions',flush=True)
def entries(body):
 # Split only top-level commas; commas inside tuples, slices or strings stay put.
 depth=0;quoted=False;escaped=False;start=0;rows=[]
 for i,c in enumerate(body):
  if quoted:
   if escaped:escaped=False
   elif c=='\\':escaped=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  elif c in '([{':depth+=1
  elif c in ')]}':depth-=1
  elif c==',' and depth==0:
   if body[start:i].strip():rows.append(body[start:i].strip())
   start=i+1
 if body[start:].strip():rows.append(body[start:].strip())
 return rows
source=P/'src/protocol.rs';test=P/'tests/protocol.rs';js=P/'scripts/protocol.cjs'
old_source,old_test,old_js=source.read_text(),test.read_text(),js.read_text()
pattern=r'pub const (\w+): \[(.+?); (\d+)\] = \[(.*?)\];'
collections=list(re.finditer(pattern,old_source,re.S));assert len(collections)==11
names=[m[1] for m in collections]
try:
 for mode in ['empty','subset','extra']:
  text=old_source
  for m in reversed(collections):
   rows=entries(m[4]);assert len(rows)==int(m[3]),m[1]
   changed=[] if mode=='empty' else (rows[:-1] if mode=='subset' else rows+[rows[0]])
   new=f'pub const {m[1]}: [{m[2]}; {len(changed)}] = ['+', '.join(changed)+'];'
   text=text[:m.start()]+new+text[m.end():]
  source.write_text(text)
  run('cardinality-protocol-native-'+mode,101,[f'APEX-CARDINALITY {n} expected/native length' for n in names])
 source.write_text(old_source)
 for mode in ['empty','subset','extra']:
  def change(m):
   rows=entries(m[1]);changed=[] if mode=='empty' else (rows[:-1] if mode=='subset' else rows+[rows[0]])
   return '&['+', '.join(changed)+']'
  # Only the four static websocket path slices, not the type declaration.
  text,n=re.subn(r'&\[((?:"[^"]+"\s*,?\s*)+)\]',change,old_source);assert n==4
  source.write_text(text)
  run('cardinality-protocol-path-'+mode,101,['ApexOmni protocol row BASE_WS_URL'])
 source.write_text(old_source)
 for bypass in [False,True]:
  if bypass:
   # Bypass only the source guard. Native length equality must still fail.
   pattern=r'assert_eq!\(\s*source.len\(\),\s*count,\s*"APEX-CARDINALITY \{name\} source total"\s*\);'
   text,n=re.subn(pattern,'let _ = count;',old_test);assert n==1
   test.write_text(text)
  for mode in ['empty','subset']:
   edit="for (const value of Object.values(tables)) if (Array.isArray(value)) value.length = "+('0' if mode=='empty' else 'value.length-1')+';\n'
   js.write_text(old_js.replace('process.stdout.write(JSON.stringify(',edit+'process.stdout.write(JSON.stringify('))
   layer='expected/native length' if bypass else 'source total'
   run(f'cardinality-protocol-source-{mode}-bypass{bypass}',101,[f'APEX-CARDINALITY {n} {layer}' for n in names])
 # Both sides empty after bypassing the source guard must still fail fixed native totals.
 text=old_source
 for m in reversed(collections):
  new=f'pub const {m[1]}: [{m[2]}; 0] = [];'
  text=text[:m.start()]+new+text[m.end():]
 source.write_text(text)
 js.write_text(old_js.replace('process.stdout.write(JSON.stringify(',
     'for (const value of Object.values(tables)) if (Array.isArray(value)) value.length=0;\nprocess.stdout.write(JSON.stringify('))
 run('cardinality-protocol-equal-empty-bypass',101,[f'APEX-CARDINALITY {n} native comparison total' for n in names])
finally:
 source.write_text(old_source);test.write_text(old_test);js.write_text(old_js)
run('cardinality-protocol-restored',0,['test result: ok.'])
