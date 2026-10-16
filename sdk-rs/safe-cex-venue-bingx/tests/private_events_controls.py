#!/usr/bin/env python3
"""One compiled mutation per newly ported private-event family."""
from pathlib import Path
import os
import subprocess

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
source=crate/'src/private_events.rs'
original=source.read_text()
env=os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(sdk/'Cargo.toml'),
         '-p','safe-cex-venue-bingx','--test','private_events','--jobs','1','--locked',
         'private_events_match_actual_typescript','--','--exact']
controls=[
 ('mapping','remaining: subtract(amount, filled)','remaining: subtract(amount, -filled)',
  'BINGX-PRIVATE native mapOrder variant 1'),
 ('fills','if increment > 0.0 {','if increment > 3.0 {',
  'BINGX-PRIVATE fill payloads handleOrderEvents variant 2'),
 ('account','notional: contracts * entry + upnl','notional: contracts * entry - upnl',
  'BINGX-PRIVATE native positions handleAccountEvents variant 0'),
 ('refresh','loaded.positions = true;','loaded.positions = false;',
  'BINGX-PRIVATE loaded flag schedulePositionRefresh variant 1'),
 ('heartbeat','send("Ping")?;','send("Pong")?;',
  'BINGX-PRIVATE calls ping variant 0'),
]
for name,before,after,assertion in controls:
 assert original.count(before)==1,f'{name}: unique production operation required'
 try:
  source.write_text(original.replace(before,after))
  broken=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (sdk/'target'/f'bingx-private-{name}-broken.log').write_text(broken.stdout)
  assert broken.returncode==101 and assertion in broken.stdout,f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
  assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
  print(f'{name}: compiled mutation exit 101 at {assertion}',flush=True)
 finally:
  source.write_text(original)
 restored=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (sdk/'target'/f'bingx-private-{name}-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
 print(f'{name}: restoration exit 0',flush=True)
assert source.read_text()==original,'production bytes restored'
