"""Sequential assertion controls for the implemented V3 signing boundary."""
from pathlib import Path
import os,subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/api.rs');source=p.read_text()
cases=[
 ('raw-string-quote','fn json_text(value: &Value) -> Result<String, String> {','fn json_text(value: &Value) -> Result<String, String> { if let Value::String(s) = value { return Ok(s.clone()); }'),
 ('number-spelling','fn json_text(value: &Value) -> Result<String, String> {','fn json_text(value: &Value) -> Result<String, String> { if value.is_object() { return serde_json::to_string(value).map_err(|e| e.to_string()); }'),
 ('index-order','safe_cex_num::JsJsonValue::from(value.clone())','safe_cex_num::JsJsonValue::from({ let mut v=value.clone(); if let Value::Object(m)=&mut v { m.sort_keys(); } v })'),
 ('uppercase','.to_uppercase()','.to_lowercase()'),
 ('proxy-url','format!("{prefix}/{url}")','format!("{url}/{prefix}")'),
 ('array-query','&format!("{prefix}[{index}]")','&format!("{prefix}{index}")'),
 ('empty-query','let url = format!("{url}?{}", query(params)?);','let url = format!("{url}{}", query(params)?);'),
 ('falsy-body','.filter(|data| truthy(data))','.filter(|data| !truthy(data))'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,old,new in cases:
 assert old in source,(name,old)
 try:
  p.write_text(source.replace(old,new,1))
  r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','signing','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-signing-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(source)
