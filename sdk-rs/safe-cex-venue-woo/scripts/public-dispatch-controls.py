from pathlib import Path
import os, subprocess
p=Path('sdk-rs/safe-cex-venue-woo/src/public_socket.rs');original=p.read_text()
changes=[
 ('disposed','if socket.is_disposed() {','if false && socket.is_disposed() {'),
 ('received','socket.track_last_received();','// removed receive observation'),
 ('control-priority','if event != "ping" && event != "pong" {','if false && event != "ping" && event != "pong" {'),
 ('own-shadow','if socket.message_handlers.borrow().contains_key(key) {','if key != "constructor" && socket.message_handlers.borrow().contains_key(key) {'),
 ('inherited-arity','member.length().is_some_and(|length| length != 0)','member.length().is_some_and(|length| length == 0)'),
 ('inherited-error','return Err(error);','drop(error); return Ok(());'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,before,after in changes:
 assert before in original,name
 try:
  p.write_text(original.replace(before,after,1))
  r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--offline','-p','safe-cex-venue-woo','--lib','public_dispatch_matches','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-dispatch-control-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout and 'assertion' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(original)
