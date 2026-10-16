from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/v3_public.rs');s=p.read_text()
changes=[('filter','row.symbol.starts_with("PERP_")','!row.symbol.starts_with("PERP_")'),('active','status == "TRADING"','status != "TRADING"'),('imr-ceiling','base_imr > 1.0','base_imr > 2.0'),('floor','(1.0 / base_imr).floor()','1.0 / base_imr'),('info','info: Some(serde_json::to_value(row).map_err(|e| e.to_string())?),','info: None,')]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,a,b in changes:
 assert a in s,a
 try:
  p.write_text(s.replace(a,b,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','v3_public','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-market-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(s)
