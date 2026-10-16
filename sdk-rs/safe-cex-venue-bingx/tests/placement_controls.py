#!/usr/bin/env python3
"""One compiled mutation per newly ported placement family."""
from pathlib import Path
import os
import sys
import subprocess

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
source=crate/'src/placement.rs'
original=source.read_text()
env=os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(sdk/'Cargo.toml'),
         '-p','safe-cex-venue-bingx','--test','placement','--jobs','1','--locked',
         'placement_matches_actual_typescript','--','--exact']
controls=[
 ('uuid','.take(40)','.take(39)','BINGX-PLACE native output createClientOrderId 1'),
 ('position-side','|| opts.reduce_only == Some(true)','|| opts.reduce_only == Some(false)','BINGX-PLACE native output getOrderPositionSide 3'),
 ('formatter','put_number(&mut payload, "quantity", lot_size)?;','put_number(&mut payload, "quantity", -lot_size)?;','BINGX-PLACE native output formatCreateOrder 0'),
 ('trailing','distance / ticker.last * 10000.0','distance / ticker.last * 1000.0','BINGX-PLACE native output formatCreateTrailingStopLossOrder 0'),
 ('parallel-wrapper','let payloads = results.into_iter().flatten().flatten().collect::<Vec<_>>();','let mut payloads = results.into_iter().flatten().flatten().collect::<Vec<_>>(); payloads.reverse();','BINGX-PLACE request sequence/body placeOrders'),
 ('submission','order.status == OrderStatus::Open','order.status == OrderStatus::Canceled','BINGX-PLACE decoded Store placeOrder'),
 ('legacy-chunks','payloads.chunks(5)','payloads.chunks(4)','BINGX-PLACE native output placeOrderBatchOld 2'),
]
if len(sys.argv)>1:
 requested=set(sys.argv[1:])
 assert requested <= {row[0] for row in controls}, 'unknown control family'
 controls=[row for row in controls if row[0] in requested]
for name,before,after,assertion in controls:
 assert original.count(before)==1,f'{name}: unique production operation required'
 try:
  source.write_text(original.replace(before,after))
  broken=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (sdk/'target'/f'bingx-placement-{name}-broken.log').write_text(broken.stdout)
  assert broken.returncode==101 and assertion in broken.stdout,f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
  assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
  print(f'{name}: compiled mutation exit 101 at {assertion}',flush=True)
 finally:
  source.write_text(original)
 restored=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (sdk/'target'/f'bingx-placement-{name}-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
 print(f'{name}: restoration exit 0',flush=True)
assert source.read_text()==original,'production bytes restored'
