#!/usr/bin/env python3
"""Serial, single-Node-per-corpus Futures ticker controls; restore every mutated byte."""
from pathlib import Path
import os, subprocess, tomllib
crate=Path(__file__).resolve().parents[1]
production=crate/'src/tickers.rs'; witness=crate/'tests/support/tickers.rs'
shared=crate.parent/'safe-cex-num/src/numeric_ingress.rs'
oracle=crate/'tests/reads-tickers-supplement.cjs'
paths=[production,witness,shared,oracle]; originals={p:p.read_bytes() for p in paths}
rows=tomllib.loads((crate.parent/'parity/manifest/exchanges/kraken/kraken.exchange.ts.toml').read_text())['symbol']
assert [(r['ts'],r['status']) for r in rows if r['rs'].startswith('safe-cex-venue-kraken/src/tickers.rs::')]==[('KrakenExchange.fetchTickers','divergent')]
controls=[]
def add(name,path,old,new,assertion,bypass=False):controls.append((name,path,old,new,assertion,bypass))
add('bid-ask-swap',production,'bid: number(&keyed(row, "bid")?)','bid: number(&keyed(row, "ask")?)','TICKER native bid')
for name,statement in [('empty','tickers.clear();'),('subset','tickers.truncate(1);'),('extra','tickers.extend(tickers.clone());')]:
 add(name,production,'        Ok(tickers)\n',f'        {statement}\n        Ok(tickers)\n','TICKER independent native length')
add('empty-guards-bypassed',production,'        Ok(tickers)\n','        tickers.clear();\n        Ok(tickers)\n','TICKER native index coverage',True)
add('trace-flatten',production,'trace(&error)?;','trace(&error).map_err(|error| ExchangeError::new(error.message))?;','TICKER trace failure allocation')
anchor='''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
empty='''        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)'''
add('consuming-empty',shared,anchor,empty,'TICKER independent native length')
add('consuming-empty-guards-bypassed',shared,anchor,empty,'TICKER native index coverage',True)
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--locked','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-kraken','--test','reads','--jobs','1','ticker_reads::']
def run():return subprocess.run(command,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
def bypass_guards():
 text=witness.read_text()
 for guard in ['''            assert_eq!(
                tickers.len(),
                count as usize,
                "TICKER independent native length"
            );
''','''            cardinality::native_len(
                tickers.len(),
                &case["result"],
                "TICKER source/native length",
            );
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
print('6 venue production + 2 consuming shared production controls; 8 named failures; 8 restored passes')
