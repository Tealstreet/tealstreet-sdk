#!/usr/bin/env python3
"""Nine compiled faults at real native public-listener consumer seams."""
from pathlib import Path
import os, subprocess, tempfile, json
workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-binance/src/public_listeners.rs'
original=source.read_bytes()
logs=Path(tempfile.mkdtemp(prefix='binance-public-listener-function-controls-'))
shared='public_listener_aliases::production_native_public_listener_functions_preserve_two_subscriptions'
single='public_listener_aliases::production_native_public_listener_aliases_wait_buffer_reuse_and_unsubscribe'
faults=[
 ('payload',b'("method".into(), string(method))',b'("__wrong_method".into(), string(method))',shared),
 ('add_handler',b'next.push(parser);',b'next.insert(0, parser);',shared),
 ('remove_handler',b'.filter(|p| !Rc::ptr_eq(p, parser))',b'.filter(|p| Rc::ptr_eq(p, parser))',shared),
 ('topic_add',b'+ 1.0;',b'+ 2.0;',shared),
 ('legacy_topic_remove',b'map.insert(topic.into(), n - 1.0);',b'map.insert(topic.into(), n + 1.0);',shared),
 ('depth',b'        symbol,\n        topic,',b'        symbol: format!("wrong-{symbol}"),\n        topic,',single),
 ('listen_futures_book',b'Venue::Futures',b'Venue::Spot',single),
 ('listen_pm_book',b'Venue::PM',b'Venue::Spot',single),
 ('listen_spot_book',b'Venue::Spot',b'Venue::Futures',single),
]
def run(test):
 p=subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-binance',
  '--features','native','--test','private_classes',test,'--','--exact','--nocapture'],
  cwd=workspace,env=os.environ.copy(),capture_output=True,text=True,timeout=900)
 return p.returncode,p.stdout+p.stderr
receipts=[]
try:
 for owner,before,after,test in faults:
  needle=f'fn {owner}('.encode();start=original.index(needle)
  # Next module item starts after this body; braces in formatting strings do not affect extraction.
  next_function=original.find(b'\nfn ',start+len(needle))
  next_public=original.find(b'\npub fn ',start+len(needle))
  ends=[n for n in [next_function,next_public] if n>=0]
  end=min(ends) if ends else len(original)
  body=original[start:end]
  assert body.count(before)==1,(owner,body.count(before))
  source.write_bytes(original[:start]+body.replace(before,after)+original[end:])
  status,output=run(test);(logs/f'{owner}-red.log').write_text(output)
  assert status!=0 and f'test {test} ... FAILED' in output and 'could not compile' not in output,output[-9000:]
  source.write_bytes(original)
  status,output=run(test);(logs/f'{owner}-restored.log').write_text(output)
  assert status==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-9000:]
  receipts.append({'owner':owner,'test':test,'compiledRed':True,'exactRestoredGreen':True})
  print(f'{owner}: named compiled RED / exact restoration GREEN',flush=True)
finally:
 source.write_bytes(original);assert source.read_bytes()==original
(logs/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
print(f'PASS {len(receipts)} listener function body controls; receipts {logs}',flush=True)
