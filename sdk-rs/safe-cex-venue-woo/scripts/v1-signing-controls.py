from pathlib import Path
import os, subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/api.rs');original=p.read_text()
head,source=original.split('// ts: signV1',1)
changes=[
 ('byte-sort','collator.compare(a, b)','a.cmp(b)'),
 ('repeat-array','let child = if repeat {', 'let child = if !repeat {'),
 ('patch-mutation','"DELETE" | "POST" | "PUT"','"DELETE" | "POST" | "PUT" | "PATCH"'),
 ('falsy-data','.get("data")\n        .filter(|v| truthy(v))','.get("data")'),
 ('empty-query','if !query.is_empty() && !mutation {','if !mutation {'),
 ('preimage-separator','format!("{query}|{}",','format!("{query}{}",'),
 ('method-preservation','let mutation = matches!', 'next.insert("method".into(), Value::String(method.clone()));\n    let mutation = matches!'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,before,after in changes:
 assert before in source,name
 try:
  p.write_text(head+'// ts: signV1'+source.replace(before,after))
  r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','signing','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-v1-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(original)
