from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/response.rs');s=p.read_text()
changes=[('funding-finite', 'Ok(Some(js_number(raw)?).filter(|v| v.is_finite()))', 'Ok(Some(js_number(raw)?))'), ('funding-blank', 'if !js_trim(s).is_empty()', 'if !s.is_empty()'), ('message-padding', '.unwrap_or(fallback)\n    .to_owned()', '.unwrap_or(fallback)\n    .trim().to_owned()'), ('seconds-scale', '(v / 1000.0).floor()', 'v.floor()'), ('millis-scale', '(v * 1000.0).floor()', 'v.floor()'), ('threshold', 'v >= 1_000_000_000_000.0', 'v > 1_000_000_000_000.0'), ('zero-timestamp', '*v >= 0.0', '*v > 0.0'), ('transaction-slash', '&s.replace(\'/\', "_")', 's'), ('requested-suffix', '[".P", "_PERP", "-PERP", "PERP"]', '[".Q", "_PERP", "-PERP", "PERP"]'), ('candidate-order', 'format!("{base}_USDT_PERP"),\n        format!("{base}-USDT-PERP"),', 'format!("{base}-USDT-PERP"),\n        format!("{base}_USDT_PERP"),')]
changes += [
 ('array-nested-priority', '.and_then(|v| v.get("rows"))', '.and_then(|_| data.get("rows"))'),
 ('array-empty-presence', '.and_then(Value::as_array)\n        .or_else', '.and_then(Value::as_array).filter(|v| !v.is_empty())\n        .or_else'),
 ('array-root-rows', '.or_else(|| data.get("rows").and_then(Value::as_array))', ''),
 ('array-nested-array', '.or_else(|| nested.and_then(Value::as_array))', ''),
 ('array-root-array', '.or_else(|| data.as_array())', ''),
]
import sys
if '--arrays' in sys.argv:
 changes = changes[-5:]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,a,b in changes:
 assert a in s,a
 try:
  p.write_text(s.replace(a,b,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','response','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-response-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(s)
