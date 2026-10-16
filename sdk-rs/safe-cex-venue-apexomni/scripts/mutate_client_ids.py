"""Compile both production helper mutations once; require each named failure."""
from pathlib import Path
import os,subprocess
root=Path(__file__).resolve().parents[3]
p=root/'sdk-rs/safe-cex-venue-apexomni/src/exchange_utils.rs'
original=p.read_text();mutated=original
controls=[('let n = "123456789";', 'let n = "987654321";', 'd8 native result'),
          ('"apexomni-{account_id}-{timestamp}-{}"',
           '"MUTATED-{account_id}-{timestamp}-{}"',
           'generateApeXRandomClientId independent native prefix')]
for before,after,_ in controls:
 assert mutated.count(before)==1,before
 mutated=mutated.replace(before,after)
cmd=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--locked','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--jobs','1','--test','utils','client_id_helpers_match_actual_typescript','--','--nocapture']
logs=root/'sdk-rs/target/apexomni-controls';logs.mkdir(parents=True,exist_ok=True)
try:
 p.write_text(mutated)
 result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);out=result.stdout+result.stderr
 (logs/'client-id-mutations.log').write_text(out)
 assert result.returncode==101 and 'test result: FAILED.' in out,out
 for _,_,label in controls:assert label in out,out
 print('2 compiling mutations: 101, both named native assertions',flush=True)
finally:p.write_text(original)
r=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
(logs/'client-id-restored.log').write_text(r.stdout+r.stderr)
assert r.returncode==0,r.stdout+r.stderr
print('restored target: 0',flush=True)
