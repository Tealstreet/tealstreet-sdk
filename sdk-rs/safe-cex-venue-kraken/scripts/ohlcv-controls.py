#!/usr/bin/env python3
"""Serial, single-Node-per-corpus Spot OHLCV controls; restore every mutated byte."""
from pathlib import Path
import os, subprocess, tomllib
crate=Path(__file__).resolve().parents[1]
production=crate/'src/spot_reads.rs'; witness=crate/'tests/support/ohlcv.rs'
shared=crate.parent/'safe-cex-num/src/numeric_ingress.rs'
oracle=crate/'tests/reads-ohlcv-supplement.cjs'
paths=[production,witness,shared,oracle]; originals={p:p.read_bytes() for p in paths}
rows=tomllib.loads((crate.parent/'parity/manifest/exchanges/kraken/krakenspot.exchange.ts.toml').read_text())['symbol']
assert [(r['ts'],r['status']) for r in rows if r['rs'].startswith('safe-cex-venue-kraken/src/spot_reads.rs::')]==[('KrakenSpotExchange.fetchOHLCV','divergent')]
controls=[]
def add(name,path,old,new,assertion,bypass=False):controls.append((name,path,old,new,assertion,bypass))
for field,old,new in [('timestamp','                timestamp,','                timestamp: timestamp / 1000.0,'),('open','open: parse(1)?','open: parse(2)?'),('high','high: parse(2)?','high: parse(3)?'),('low','low: parse(3)?','low: parse(4)?'),('close','close: parse(4)?','close: parse(6)?'),('volume','volume: parse(6)?','volume: parse(5)?')]:
 add('field-'+field,production,old,new,'OHLCV native '+field)
for name,statement in [('empty','candles.clear();'),('subset','candles.truncate(1);'),('extra','candles.extend(candles.clone());')]:
 add(name,production,'        Ok(candles)\n',f'        {statement}\n        Ok(candles)\n','OHLCV independent native length')
add('empty-guards-bypassed',production,'        Ok(candles)\n','        candles.clear();\n        Ok(candles)\n','OHLCV native index coverage',True)
add('query-since',production,'value / 1000.0','value / 1.0','OHLCV native query fields')
add('query-interval',production,'TableHit::Mapped(*value)','TableHit::Mapped(*value * 2.0)','OHLCV native query fields')
add('sort-direction',production,'left.timestamp - right.timestamp','right.timestamp - left.timestamp','OHLCV native timestamp')
add('console-flatten',production,'&error.message)?;','&error.message).map_err(|error| ExchangeError::new(error.message))?;','OHLCV console error allocation')
anchor='''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
empty='''        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)'''
add('consuming-empty',shared,anchor,empty,'OHLCV independent native length')
add('consuming-empty-guards-bypassed',shared,anchor,empty,'OHLCV native index coverage',True)
for name,replacement in [('source-empty','[]'),('source-subset','cases.slice(1)')]:
 add(name,oracle,'ohlcv:cases,','ohlcv:'+replacement+',','KRAKEN-CARDINALITY source total and per-case counts reads')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--locked','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-kraken','--test','reads','--jobs','1','ohlcv::']
def run():return subprocess.run(command,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
def bypass_guards():
 text=witness.read_text()
 for guard in ['''            assert_eq!(
                candles.len(),
                expected_count as usize,
                "OHLCV independent native length"
            );
''','''            cardinality::native_len(candles.len(), &case["result"], "OHLCV source/native length");
''']:
  assert text.count(guard)==1,'guard anchor';text=text.replace(guard,'')
 witness.write_text(text)
base=run();assert base.returncode==0,base.stdout
for name,path,old,new,assertion,bypass in controls:
 try:
  text=originals[path].decode();assert text.count(old)==1,(name,'anchor',text.count(old));path.write_text(text.replace(old,new))
  if bypass:bypass_guards()
  broken=run();assert broken.returncode==101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout,(name,broken.stdout)
  print(f'{name}: compiled; exit 101; {assertion}',flush=True)
 finally:
  for path,data in originals.items():path.write_bytes(data)
 restored=run();assert restored.returncode==0,(name,restored.stdout)
 assert all(p.read_bytes()==data for p,data in originals.items())
 print(f'{name}: restored pass',flush=True)
print('14 venue production + 2 consuming shared production + 2 source witness controls; 18 named failures; 18 restored passes')
