from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/pricing.rs');s=p.read_text()
changes=[('positive-finite', 'p.is_finite() && p > 0.0', 'p > 0.0'), ('nan-clamp', 'if value.is_nan() {', 'if value.is_nan() && false {'), ('slip-side', '1.0 - factor', '1.0 + factor'), ('fair-priority', '[Some(t.mark), Some(t.index), t.mid_price, Some(t.last)]', '[Some(t.last), Some(t.index), t.mid_price, Some(t.mark)]'), ('cap', 'if within && misses {', 'if (within || !within) && misses {'), ('misses-touch', 'if within && misses {', 'if within && (misses || !misses) {'), ('range-fallback', '} else {\n        1.0\n    })', '} else {\n        2.0\n    })'), ('rate-status', '429.0', '428.0'), ('rate-code', '-1003.0', '-1002.0'), ('rate-alt-code', '90003.0', '90002.0'), ('mode-message', 'contains("position mode remains unchanged")', 'contains("position mode has changed")'), ('cross-equal', 'price >= touch_price', 'price > touch_price'), ('cross-fair', 'is_post_only_crossing_price(side, price, fair)', 'is_post_only_crossing_price(side, price + 1.0, fair)'), ('coercion-error', 'js_string(selected.or(Some(&Value::String(String::new()))))', 'js_string(selected.or(Some(&Value::String(String::new())))).or(Ok(String::new()))')]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,a,b in changes:
 assert a in s,a
 try:
  p.write_text(s.replace(a,b,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','pricing','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-pricing-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(s)
