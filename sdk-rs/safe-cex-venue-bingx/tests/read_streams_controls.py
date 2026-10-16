#!/usr/bin/env python3
"""One compiled production mutation for each new read/stream family."""
from pathlib import Path
import os
import subprocess

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
env=os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(sdk/'Cargo.toml'),
 '-p','safe-cex-venue-bingx','--test','read_streams','--jobs','1','--locked',
 'read_streams_match_actual_typescript','--','--exact']
controls=[
 ('orders','read_history.rs','order.r#type = OrderType::TakeProfit;','order.r#type = OrderType::Limit;','BINGX-RS decoded native output fetchOrders 0'),
 ('candles','read_history.rs','timestamp: number(row.get("time"))? / 1000.0,','timestamp: number(row.get("time"))? / 100.0,','BINGX-RS decoded native output fetchOHLCV 0'),
 ('fills','read_history.rs','commission: Some(commission),','commission: Some(-commission),','BINGX-RS decoded native output fetchHistoricalFills 0'),
 ('forwarding','stream_subscriptions.rs','    listen(opts, callback)\n','    let result=listen(opts, callback); drop(std::hint::black_box(result)); panic!("mutated forwarding return")\n','BINGX-RS native call completed exchange.listenOHLCV'),
 ('book','stream_subscriptions.rs','retained.borrow_mut().asks = asks;','retained.borrow_mut().bids = asks;','BINGX-RS decoded native output public.listenOrderBook 0'),
 ('private-routing','private_dispatch.rs','send_with_log("Pong")','send_with_log("Ping")','BINGX-RS calls private.onMessage 0'),
 ('polling','read_history.rs','(count + 2.0) % 4.0 == 0.0','(count + 1.0) % 4.0 == 0.0','BINGX-RS decoded Store tick'),
 ('disposal','read_history.rs','    base()?;\n    public()?;','    public()?;\n    base()?;','BINGX-RS calls dispose 0'),
]
for name,file,before,after,assertion in controls:
 source=crate/'src'/file
 original=source.read_text()
 assert original.count(before)==1,f'{name}: one production operation required'
 try:
  source.write_text(original.replace(before,after))
  broken=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (sdk/'target'/f'bingx-read-streams-{name}-broken.log').write_text(broken.stdout)
  assert broken.returncode==101 and assertion in broken.stdout,f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
  assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
  print(f'{name}: compiled mutation exit 101 at {assertion}',flush=True)
 finally:
  source.write_text(original)
 restored=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (sdk/'target'/f'bingx-read-streams-{name}-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
 print(f'{name}: restoration exit 0',flush=True)
 assert source.read_text()==original,'production bytes restored'
