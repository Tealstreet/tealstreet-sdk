#!/usr/bin/env python3
"""Mutate each real SDK inherited-refusal method, require compiled named RED,
restore exact source bytes and require its named native test GREEN.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

if not __debug__:
 raise SystemExit("inherited refusal controls require Python assertions; do not use -O")

ROOT = Path(__file__).resolve().parents[2]
SOURCE_FILE = ROOT / 'safe-cex-venue-binance/src/exchange_trait.rs'
CASES = [
 ('BinanceExchange','fetch_symbol_max_leverage','futures_max_leverage'),
 ('BinancePMExchange','fetch_symbol_max_leverage','pm_max_leverage'),
 ('BinancePMExchange','fetch_public_trades','pm_public_trades'),
 ('BinanceSpotExchange','fetch_symbol_max_leverage','spot_max_leverage'),
 ('BinanceSpotExchange','fetch_public_trades','spot_public_trades'),
 ('BinanceSpotExchange','set_leverage','spot_leverage'),
 ('BinanceSpotExchange','change_position_mode','spot_position_mode'),
 ('BinanceSpotExchange','change_margin_type','spot_margin_type'),
]
COMMAND = ['cargo','test','--locked','-p','safe-cex-venue-binance','--features','native','--test','inherited_refusal_binding','--jobs','1']
ENV = dict(os.environ,CARGO_BUILD_JOBS='1')
def run(label,test=None,include_source=False):
 log=Path('/tmp/port-txflow-inherited-control-'+label+'.log')
 cmd=COMMAND + ([test,'--','--exact'] if test else ['--','--include-ignored'] if include_source else [])
 with log.open('w') as output:
  result=subprocess.run(cmd,cwd=ROOT,env=ENV,stdout=output,stderr=subprocess.STDOUT)
 return result.returncode,log.read_text()
status,log=run('baseline')
assert status==0 and '8 passed; 0 failed' in log,log[-5000:]
original=SOURCE_FILE.read_bytes()
receipts=[]
for owner,method,label in CASES:
 text=original.decode()
 impl_start=text.index('impl Exchange for '+owner+' {')
 next_impl=text.find('\nimpl Exchange for ',impl_start+1)
 impl_end=len(text) if next_impl<0 else next_impl
 fn_start=text.index('    fn '+method+'(',impl_start,impl_end)
 next_fn=text.find('\n    fn ',fn_start+1,impl_end)
 fn_end=impl_end if next_fn<0 else next_fn
 body=text[fn_start:fn_end]
 assert body.count('base_exchange_rejection()')==1,(owner,method)
 test=label+'_preserves_inherited_refusal'
 try:
  replacement='Box::pin(async { Err(ExchangeError::new("changed inherited refusal")) })'
  SOURCE_FILE.write_text(text[:fn_start]+body.replace('base_exchange_rejection()',replacement)+text[fn_end:])
  status,log=run(label+'-red',test)
  assert status==101 and not re.search(r'^error(?:\[|: could not compile)',log,re.M),log[-5000:]
  assert 'test '+test+' ... FAILED' in log and '1 failed' in log,log[-5000:]
 finally:
  SOURCE_FILE.write_bytes(original)
 assert SOURCE_FILE.read_bytes()==original
 status,log=run(label+'-restored',test)
 assert status==0 and '1 passed; 0 failed' in log,log[-5000:]
 receipts.append({'owner':owner,'method':method,'compiledNamedRed':True,'exactRestoredGreen':True})
 print(owner+'::'+method+' compiled RED -> exact restored GREEN',flush=True)
status,log=run('final',include_source=True)
assert status==0 and '9 passed; 0 failed' in log,log[-5000:]
assert SOURCE_FILE.read_bytes()==original
Path('/tmp/port-txflow-inherited-all-controls.json').write_text(json.dumps({'controls':receipts,'sourceSha256':hashlib.sha256(original).hexdigest()},indent=2)+'\n')
