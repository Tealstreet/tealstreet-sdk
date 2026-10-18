"""Sequential compiling controls for public ticker writers; restores production."""
from pathlib import Path
import os, subprocess, sys
path=Path('sdk-rs/safe-cex-venue-woo/src/public_socket.rs')
original=path.read_text()
cases=[
 ('notification-id-loss', 'ExchangeError::from(error)', '{ let mut projected = ExchangeError::from(error); projected.store_subscriber_ids.clear(); projected }', 'writer retains one Store-local subscriber identity'),
 ('notification-flatten', 'ExchangeError::from(error)', 'ExchangeError::new(error.to_string())', 'subscriber cause identity survives writer'),
 ('notification-swallow', 'updated.map_err(|error: StoreMutationError| ExchangeError::from(error))?;', 'if updated.is_err() { return Ok(()); }', 'subscriber failure must propagate'),
 ('ticker-field', '("last", "close")', '("last", "amount")', 'writer native bits'),
 ('bbo-field', '("bid", "bid")', '("bid", "ask")', 'writer native bits'),
 ('mark-field', '("mark", "price")', '("mark", "bid")', 'writer native bits'),
 ('spot-filter', '!symbol.starts_with(&"PERP_".encode_utf16().collect::<Vec<_>>())', '!symbol.starts_with(&"SPOT_".encode_utf16().collect::<Vec<_>>())', 'writer notification order'),
 ('truncate-batch', 'for row in rows {', 'for row in rows.iter().take(1) {', 'writer notification order'),
 ('eager-refusal', "continue 'rows;", 'if !rows.is_empty() { return Err(ExchangeError::new(refusals.join("; "))); } continue \'rows;', 'later source error wins over typed refusal'),
 ('zero-sign', 'patch.set_number(*target, number);', 'patch.set_number(*target, if number == 0.0 { 0.0 } else { number });', 'writer native bits'),
]
assert not (set(sys.argv[1:]) - ({case[0] for case in cases} | {'parent-hook-swallow'})), 'unknown control selector'
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,before,after,assertion in cases:
 if len(sys.argv)>1 and name not in sys.argv[1:]:continue
 assert before in original,name
 try:
  path.write_text(original.replace(before,after,1))
  run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--offline','--jobs','1','-p','safe-cex-venue-woo','--lib','public_writer_matches'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-writer-control-'+name+'.log').write_text(run.stdout)
  ok=run.returncode==101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
  print(name,run.returncode,'assertion-red' if ok else 'BAD',flush=True)
  assert ok,run.stdout
 finally:path.write_text(original)

if len(sys.argv) == 1 or 'parent-hook-swallow' in sys.argv[1:]:
 path=Path('sdk-rs/safe-cex-venue-woo/tests/support/ws.rs')
 original=path.read_text()
 before='Some((selected, error)) if *selected == hook => Err(error.clone()),'
 assert original.count(before)==1
 try:
  path.write_text(original.replace(before, 'Some((selected, _error)) if *selected == hook => Ok(()),'))
  run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--offline','--jobs','1','-p','safe-cex-venue-woo','--test','public_heartbeats','parent_hooks_retain_injected_failure_identity'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-writer-control-parent-hook-swallow.log').write_text(run.stdout)
  ok=run.returncode==101 and 'test result: FAILED' in run.stdout and 'log hook retains failure identity' in run.stdout
  print('parent-hook-swallow',run.returncode,'assertion-red' if ok else 'BAD',flush=True)
  assert ok,run.stdout
 finally:path.write_text(original)
