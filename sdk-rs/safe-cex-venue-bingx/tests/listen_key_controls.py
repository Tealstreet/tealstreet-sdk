#!/usr/bin/env python3
"""One compiled mutation per new listen-key lifecycle family."""
from pathlib import Path
import os
import subprocess

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
source=crate/'src/listen_key.rs'
original=source.read_text()
env=os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(sdk/'Cargo.toml'),
         '-p','safe-cex-venue-bingx','--test','listen_key','--jobs','1','--locked',
         'listen_key_matches_actual_typescript','--','--exact']
controls=[
 ('acquisition','.filter(|v| js_truthy(Some(v)))','.filter(|_| false)',
  'BINGX-KEY native fetchListenKey variant 0'),
 ('renewal','self.rebuilding.set(false);','self.rebuilding.set(true);',
  'BINGX-KEY native state updateListenKey variant 1'),
 ('timing','delay.unwrap_or(30.0 * 60.0 * 1000.0)','delay.unwrap_or(30.0 * 60.0 * 100.0)',
  'BINGX-KEY operation order onOpen variant 0'),
 ('clearing','&& truthy(&timer)','&& !truthy(&timer)',
  'BINGX-KEY operation order connectAndSubscribe variant 0'),
]
for name,before,after,assertion in controls:
 assert original.count(before)==1,f'{name}: unique production operation required'
 try:
  source.write_text(original.replace(before,after))
  broken=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (sdk/'target'/f'bingx-listen-{name}-broken.log').write_text(broken.stdout)
  assert broken.returncode==101 and assertion in broken.stdout,f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
  assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
  print(f'{name}: compiled mutation exit 101 at {assertion}',flush=True)
 finally:
  source.write_text(original)
 restored=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (sdk/'target'/f'bingx-listen-{name}-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
 print(f'{name}: restoration exit 0',flush=True)
assert source.read_text()==original,'production bytes restored'
