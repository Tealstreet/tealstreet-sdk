#!/usr/bin/env python3
"""One compiling fault injection for each new journal test; serial Cargo only."""
from pathlib import Path
import os,subprocess
root=Path(__file__).resolve().parents[3]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
print('candidate SHA',subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),flush=True)
controls=[('position_intervals.rs','position.saw_missing_realized_pnl = true;','position.saw_missing_realized_pnl = false;','journal_interval_native_source_parity','JOURNAL native absent interval realizedPnl'),('build_records.rs','options.from - STRADDLE_LOOKBACK_MS','options.from + STRADDLE_LOOKBACK_MS','journal_build_native_source_parity','BUILD native lookback')]
base=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-exchange','--test','journal','--jobs','2']
for filename,before,after,test,assertion in controls:
 path=root/'sdk-rs/safe-cex-exchange/src'/filename;original=path.read_text();assert original.count(before)==1
 try:
  path.write_text(original.replace(before,after));r=subprocess.run(base+[test,'--','--include-ignored'],cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  print(filename,'exit',r.returncode,flush=True);print(r.stdout,flush=True)
  assert r.returncode==101 and assertion in r.stdout and 'panicked at' in r.stdout,'survivor or non-runtime failure'
 finally:path.write_text(original)
r=subprocess.run(base+['--','--include-ignored'],cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
print('restored exit',r.returncode,flush=True);print(r.stdout,flush=True)
assert r.returncode==0 and '2 passed; 0 failed; 0 ignored' in r.stdout
