#!/usr/bin/env python3
"""Eight compiled family defects, ten named failures, exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/ws_controls.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('table[usize::from(testnet)].1','table[usize::from(!testnet)].1',('public_connect_native','private_connect_native'),'native UTF-16'),
 ('["tickers", "bbos", "markprices", "estfundingrate"]','["tickers", "WRONG", "markprices", "estfundingrate"]',('public_open_native',),'native UTF-16'),
 ('("orderly_key", key)','("WRONG", key)',('private_open_native',),'native object key'),
 ('let topics = h.flatten_one(values)?;','let topics = values;',('public_subscribe_native',),'native'),
 ('["executionreport", "algoexecutionreportv2", "position"]','["executionreport", "WRONG", "position"]',('private_subscribe_native',),'native UTF-16'),
 ('frame(h, &r, method, vec![("event", text(h, "ping"))])','frame(h, &r, method, vec![("event", text(h, "WRONG"))])',('public_ping_native','private_ping_native'),'native UTF-16'),
 ('frame(h, &r, "send", vec![("event", text(h, "pong"))])','frame(h, &r, "send", vec![("event", text(h, "WRONG"))])',('public_pong_native',),'native UTF-16'),
 ('if h.includes(&data, &event_prefix)? || h.includes(&data, &topic_prefix)? {','if false && (h.includes(&data, &event_prefix)? || h.includes(&data, &topic_prefix)?) {',('public_message_native',),'native array cardinality'),
]
for old,new,tests,assertion in controls:
 assert source.count(old)==1,old
def run(tests=None):
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','ws_controls','--jobs','2']
 if tests:
  if len(tests)==1: command.extend([tests[0],'--','--exact'])
  else: command.append('connect_native' if 'connect' in tests[0] else 'ping_native')
 return subprocess.run(command,env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
for old,new,tests,assertion in controls:
 try:
  p.write_text(source.replace(old,new));broken=run(tests)
  assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
  for test in tests:
   assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
   failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
   assert assertion in failure,(test,failure)
   print(f'{test}: compiled native assertion exit 101 ({assertion})',flush=True)
 finally:p.write_bytes(original)
restored=run();assert restored.returncode==0,restored.stdout
for old,new,tests,assertion in controls:
 for test in tests:
  assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
  print(f'{test}: restored exit 0',flush=True)
assert p.read_bytes()==original
print(f'{revision}: eight individually compiled family defects, ten named failures/restored passes, byte-exact restoration')
