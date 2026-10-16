from pathlib import Path
import os, subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/factory.rs');original=p.read_text()
changes=[
 ('public-bypass','if url.contains("/public/") {','if false && url.contains("/public/") {'),
 ('broker-unsigned-return','_ => return Ok(Value::Object(next)),','_ => {},'),
 ('remote-bypass','if remote_signer || options.key.is_empty() || options.secret.is_empty() {','if (false && remote_signer) || options.key.is_empty() || options.secret.is_empty() {'),
 ('array-broker-property','array_property = Some(key);','array_property = Some(key).filter(|_| false);'),
 ('version-substring','if url.contains("v1") {','if url.starts_with("/v1/") {'),
 ('testnet-broker','let broker = if options.testnet == Some(true) {','let broker = if options.testnet == Some(false) {'),
 ('zero-timeout','timeout: timeout as i64,','timeout: if timeout == 0.0 { 20000 } else { timeout as i64 },'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,before,after in changes:
 assert before in original,name
 try:
  p.write_text(original.replace(before,after,1))
  r=subprocess.run(['cargo','test','--locked','--offline','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--lib','factory','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-factory-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(original)
