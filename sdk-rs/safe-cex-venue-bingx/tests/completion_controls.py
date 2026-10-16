#!/usr/bin/env python3
"""One compiled production mutation per completion family; restore after each."""
from pathlib import Path
import os
import subprocess
import sys

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
env=os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(sdk/'Cargo.toml'),'-p','safe-cex-venue-bingx','--test','completion','--jobs','1','--locked']
controls=[
 ('api','api.rs','RetainedJsonInput::Float(self.host.now_ms())','RetainedJsonInput::Float(self.host.now_ms()+1.0)','api_factory_matches_actual_typescript','BINGX-C API prepared 0'),
 ('exchange-constructor','exchange.rs','secondary(&options, &factory, 3.0)','secondary(&options, &factory, 4.0)','completion_matches_actual_typescript','BINGX-C calls exchangeClass 0'),
 ('startup','exchange.rs','if public_only {\n            let mut loaded = self.base.store.loaded().clone();\n            loaded.balance = true;','if public_only {\n            let mut loaded = self.base.store.loaded().clone();\n            loaded.balance = false;','completion_matches_actual_typescript','BINGX-C loaded start'),
 ('quantity','amendment.rs','Ok(vec![Some(json!(order.id))])','Ok(vec![Some(json!("wrong-order"))])','completion_matches_actual_typescript','BINGX-C output updateOrder 0'),
 ('trigger','amendment.rs','.cancel_orders(std::slice::from_ref(order))','.cancel_orders(&[])','completion_matches_actual_typescript','BINGX-C direct native order status'),
 ('cancel-replace','amendment.rs','.update_order_status(&reconciled.id, OrderStatus::Canceled)','.update_order_status(&reconciled.id, OrderStatus::Open)','completion_matches_actual_typescript','BINGX-C direct native order status'),
 ('public-class','websocket.rs','let stream = BingxPublicStream::new(reads, socket);','let stream = BingxPublicStream::new(reads, socket);\n        stream.topics.borrow_mut().push(("ghost-topic".into(),1.0));','completion_matches_actual_typescript','BINGX-C output exchangeClass 0'),
 ('private-constructor','websocket.rs','.clear_timer()?;','.schedule_listen_key_refresh(None)?;','completion_matches_actual_typescript','BINGX-C output exchangeClass 0'),
]
selected=set(sys.argv[1:])
assert selected <= {control[0] for control in controls}, "unknown control family"
for name,file,before,after,test,assertion in controls:
 if selected and name not in selected:
  continue
 source=crate/'src'/file
 original=source.read_text()
 assert original.count(before)==1,f'{name}: unique production operation'
 try:
  source.write_text(original.replace(before,after))
  broken=subprocess.run(command+[test,'--','--exact'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (sdk/'target'/f'bingx-completion-{name}-broken.log').write_text(broken.stdout)
  assert broken.returncode==101 and assertion in broken.stdout,f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
  assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
  print(f'{name}: compiled mutation exit 101 at {assertion}',flush=True)
 finally:
  source.write_text(original)
 restored=subprocess.run(command+[test,'--','--exact'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (sdk/'target'/f'bingx-completion-{name}-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,f'{name}: restored exit {restored.returncode}\n{restored.stdout}'
 assert source.read_text()==original,'production bytes restored'
 print(f'{name}: restoration exit 0',flush=True)
