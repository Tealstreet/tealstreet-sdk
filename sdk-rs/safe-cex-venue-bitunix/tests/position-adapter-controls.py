#!/usr/bin/env python3
"""One native field fault injection for this new adapter test, then exact restoration."""
from pathlib import Path
import os,subprocess
root=Path(__file__).resolve().parents[3]
path=root/'sdk-rs/safe-cex-venue-bitunix/src/position_adapter.rs';original=path.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitunix','--test','position_adapter','--jobs','2','--','--include-ignored']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
print('candidate SHA',subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),flush=True)
before='        funding,';after='        funding: -funding,';assertion='JOURNAL native funding'
assert original.count(before)==1
try:
 path.write_text(original.replace(before,after));r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 print('mutated exit',r.returncode,flush=True);print(r.stdout,flush=True)
 assert r.returncode==101 and assertion in r.stdout and 'panicked at' in r.stdout,'survivor or non-runtime failure'
finally:path.write_text(original)
r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
print('restored exit',r.returncode,flush=True);print(r.stdout,flush=True)
assert r.returncode==0 and '1 passed; 0 failed; 0 ignored' in r.stdout
