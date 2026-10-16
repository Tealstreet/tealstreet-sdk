"""One corpus per sequential raw-read production control; require runtime failure."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/raw_reads.rs'
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','raw_reads','--jobs','1']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
('instruments endpoint','"instruments/query"','"wrong-instruments"','raw_read_fetchInstrumentsRaw'),
('details endpoint','"accounts/{accountCode}/instruments/query"','"wrong-details"','raw_read_fetchInstrumentDetailsRaw'),
('portfolio endpoint','"accounts/{accountCode}/portfolio"','"wrong-portfolio"','raw_read_fetchPortfolioRaw'),
('filter predicate','!= Some("XBTUSD")','== Some("XBTUSD")','raw_read_fetchInstrumentsRaw'),
('error truthiness','if js_truthy(message)','if message.is_some()','raw_read_fetchInstrumentsRaw'),
('swallowed logger','report(selected)?;','let _ = report(selected);','raw_read_fetchInstrumentsRaw'),
]
for name,old,new,assertion in controls:
 original=source.read_bytes();s=original.decode();assert s.count(old)==1,(name,s.count(old))
 try:
  mutated=s.replace(old,new)
  if name=='error truthiness':mutated=mutated.replace('js_number_output, js_truthy,','js_number_output,')
  source.write_text(mutated)
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: FAIL exit=101 ({assertion})',flush=True)
 finally:source.write_bytes(original)
 r=subprocess.run(cmd,env=env,capture_output=True,text=True)
 assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)
