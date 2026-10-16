from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/utils.rs');s=p.read_text()
changes=[('prefix', 'matches!(parts[0], "PERP" | "SPOT")', 'matches!(parts[0], "PERP")'), ('reverse-suffix', '[".P", "_PERP", "-PERP", "PERP", "USDT"]', '[".P", "_PERP", "-PERP", "PERP", "USDC"]'), ('hedge-close', '"CLOSE LONG" => Some((OrderSide::Sell, PositionSide::Long))', '"CLOSE LONG" => Some((OrderSide::Buy, PositionSide::Long))'), ('whitespace', 'safe_cex_num::js_split_whitespace(&upper)', 'upper.split_whitespace().collect::<Vec<_>>()'), ('colon-tail', "upper\n        .rsplit(':')", "upper\n        .split(':')")]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,a,b in changes:
 assert a in s,a
 try:
  p.write_text(s.replace(a,b,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','utils','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-utils-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(s)
