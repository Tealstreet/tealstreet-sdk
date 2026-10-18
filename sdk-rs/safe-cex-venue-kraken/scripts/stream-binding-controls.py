#!/usr/bin/env python3
"""Compile real native binding faults; never count build failures as RED."""
from pathlib import Path
import os, subprocess, tempfile, json
crate=Path(__file__).resolve().parents[1]
production=crate/'src/binding/members.rs'
original=production.read_bytes()
env={**os.environ,'CARGO_INCREMENTAL':'0'}
assert env.get('SAFE_CEX_TYPESCRIPT_PATH')
receipts=Path(tempfile.mkdtemp(prefix='kraken-stream-binding-controls-'))
controls=[
 ('book','--test','exchange_binding','constructed_public_book_frames_keep_source_string_iterables',
  'delegate_public(&rt, "listenOrderBook", vec![(*a0).clone(), (*a1).clone()])',
  'delegate_public(&rt, "listenOHLCV", vec![(*a0).clone(), (*a1).clone()])'),
 ('candle','--test','exchange_binding','constructed_spot_candles_keep_source_date_input_coercion',
  'delegate_public(&rt, "listenOHLCV", vec![(*a0).clone(), (*a1).clone()])',
  'delegate_public(&rt, "listenOrderBook", vec![(*a0).clone(), (*a1).clone()])'),
 ('cleanup','--lib',None,'futures_public_candle_members_keep_live_source_noop_contract',
  '("stream_adapters", "futures_listen_ohlcv") => value(disposer(|| Ok(()))),',
  '("stream_adapters", "futures_listen_ohlcv") => value(Js::Null),'),
 ('handler','--lib',None,'futures_public_candle_members_keep_live_source_noop_contract',
  '("stream_adapters", "futures_handle_ohlc") => value(Js::Undefined),',
  '("stream_adapters", "futures_handle_ohlc") => value(Js::Null),'),
]
rows=[]
for label,kind,target,test,old,new in controls:
 assert original.decode().count(old)==1, label
 args=['cargo','test','--locked','-p','safe-cex-venue-kraken','--jobs','1',kind]+([target] if target else [])+[test]
 def run(stage):
  r=subprocess.run(args,cwd=crate.parent,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (receipts/f'{label}-{stage}.log').write_text(r.stdout)
  return r
 try:
  production.write_text(original.decode().replace(old,new))
  red=run('red')
  assert red.returncode==101 and 'test result: FAILED' in red.stdout and f'{test} ... FAILED' in red.stdout and 'panicked at' in red.stdout, red.stdout
 finally:
  production.write_bytes(original)
 green=run('restored')
 assert green.returncode==0 and f'{test} ... ok' in green.stdout, green.stdout
 assert production.read_bytes()==original
 rows.append({'control':label,'named_test':test,'compiled_red':True,'exact_restored_green':True})
 (receipts/'receipts.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(f'{label}: named compiled RED -> exact restored GREEN',flush=True)
print(receipts,flush=True)
