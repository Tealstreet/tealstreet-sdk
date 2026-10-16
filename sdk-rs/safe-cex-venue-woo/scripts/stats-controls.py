from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/v3_public.rs');s=p.read_text()
changes=[('math-round', 'safe_cex_num::math_round(((close24h - open24h) / open24h) * 10_000.0)', '(((close24h - open24h) / open24h) * 10_000.0).round()'), ('prototype', 'TableHit::Inherited(_) => None,', 'TableHit::Inherited(_) => continue,'), ('lookup-precedence', '        let mut hit = js_table_lookup(&entries, &normalized);\n        // Source uses ??, not membership or truthiness. Typed Market values are non-null.\n        if hit.is_nullish_by(|_| false) {\n            hit = js_table_lookup(&entries, &row.symbol);\n        }\n', '        let mut hit = js_table_lookup(&entries, &row.symbol);\n        // Source uses ??, not membership or truthiness. Typed Market values are non-null.\n        if hit.is_nullish_by(|_| false) {\n            hit = js_table_lookup(&entries, &normalized);\n        }\n'), ('raw-fallback', 'hit = js_table_lookup(&entries, &row.symbol);', 'hit = TableHit::Missing;'), ('positive-open', 'number(&row.open_24h, "24hOpen", f64::from_bits(1))', 'number(&row.open_24h, "24hOpen", 0.0)'), ('required-string', 'String::deserialize(d).map(Some)', 'Option::<String>::deserialize(d)'), ('percentage-scale', '/ 100.0,', '/ 1000.0,')]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,a,b in changes:
 assert a in s,a
 try:
  p.write_text(s.replace(a,b,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','v3_public','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-stats-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(s)
