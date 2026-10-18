#!/usr/bin/env python3
"""Require named real native socket failures, then restore exact production bytes."""
from pathlib import Path
import os, subprocess, tempfile, json
workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-binance/src/orderbook.rs'
original = source.read_bytes()
logs = Path(tempfile.mkdtemp(prefix='binance-orderbook-function-controls-'))
listener = 'public_listener_aliases::production_native_public_listener_aliases_wait_buffer_reuse_and_unsubscribe'
cases = [
 ('load_binance_order_book_snapshot',b'state.snapshot_last_update_id = Some(last_update_id);',b'state.snapshot_last_update_id = Some(-last_update_id);','private_classes',listener),
 ('get_buffered_binance_depth_updates',b'js_number(update.get("u"))? > snapshot_last_update_id',b'js_number(update.get("u"))? < snapshot_last_update_id','inverse_socket_binding','production_inverse_depth_overflow_ids_keep_native_sequence_rules'),
 ('apply_binance_depth_update',b'if !final_id.is_finite() {',b'if final_id.is_nan() {','private_classes','public_listener_aliases::production_native_depth_overflow_final_id_requests_resnapshot'),
 ('depth_index',b'rows.get(index as usize).cloned()',b'rows.get(index as usize + 1).cloned()','private_classes',listener),
]
def run(target,test):
 p = subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-binance',
  '--features','native','--test',target,test,'--','--exact','--nocapture'],
  cwd=workspace,env=os.environ.copy(),capture_output=True,text=True,timeout=900)
 return p.returncode,p.stdout+p.stderr
receipts=[]
try:
 for owner,before,after,target,test in cases:
  assert original.count(before)==1,owner
  source.write_bytes(original.replace(before,after))
  status,output=run(target,test)
  (logs/f'{owner}-red.log').write_text(output)
  assert status!=0 and f'test {test} ... FAILED' in output and 'could not compile' not in output,output[-9000:]
  source.write_bytes(original)
  status,output=run(target,test)
  (logs/f'{owner}-restored.log').write_text(output)
  assert status==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-9000:]
  receipts.append({'owner':owner,'target':target,'test':test,'compiledRed':True,'exactRestoredGreen':True})
  print(f'{owner}: named compiled RED / exact restoration GREEN',flush=True)
finally:
 source.write_bytes(original)
 assert source.read_bytes()==original
(logs/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
print(f'PASS {len(receipts)} orderbook body controls; receipts {logs}',flush=True)
