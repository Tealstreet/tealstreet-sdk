"""Compile every production control, demand a named runtime assertion, restore/pass."""
from pathlib import Path
import argparse,os,subprocess
parser=argparse.ArgumentParser();parser.add_argument('--start-at',type=int,default=0);parser.add_argument('--only');parser.add_argument('--check-anchors',action='store_true');args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/quotes.rs';test=crate/'tests/quotes.rs'
cmd=['cargo','test','-p','safe-cex-venue-breakoutprop','--test','quotes','--jobs','1','actual_quote_reads','--','--exact']
env=dict(os.environ)
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH',subprocess.check_output(['node','-p',"require.resolve('typescript')"],cwd=crate.parent,text=True).strip())
controls=[
 ('markets base projection','                    base,','                    base: format!("{base}wrong"),','QUOTES-NATIVE fetchMarkets',False),
 ('amount precision','amount: market_property(&details, "minOrderSizeIncrement")?','amount: market_property(&instrument, "priceIncrement")?','QUOTES-NATIVE fetchMarkets',False),
 ('price precision','price: market_property(&instrument, "priceIncrement")?','price: market_property(&details, "minOrderSizeIncrement")?','QUOTES-NATIVE fetchMarkets',False),
 ('ticker endpoint','url: "marketdata".into()','url: "wrong-marketdata".into()','QUOTES-REQUEST fetchTickersForSymbols',False),
 ('wrapper symbol order','store.markets().iter().map(|m| m.symbol.clone()).collect()','store.markets().iter().rev().map(|m| m.symbol.clone()).collect()','QUOTES-REQUEST fetchTickers',False),
]
controls += [(f'ticker {field}',old,new,'QUOTES-NATIVE fetchTickersForSymbols',False) for field,old,new in [
 ('bid','                bid,','                bid: ask,'),
 ('ask','                ask,','                ask: bid,'),
 ('last','last: bid,','last: ask,'),('mark','mark: bid,','mark: ask,'),('index','index: bid,','index: ask,')]]
for name,anchor in [('markets','Ok(markets)\n        })();'),('tickers','Ok(Some(tickers))')]:
 for shape,code in [('empty',f'{name}.clear();'),('subset',f'{name}.truncate(1);'),('extra',f'if let Some(row)={name}.first().cloned(){{{name}.push(row);}}')]:
  controls.append((name+' '+shape,anchor,code+anchor,'NATIVE-CARDINALITY expected/native length',True))
controls += [('undefined becomes empty','Ok(None) => return Ok(None),','Ok(None) => return Ok(Some(vec![])),','QUOTES-NATIVE fetchTickersForSymbols',False),('swallow ticker reporter','report(error, logger)?;','let _ = report(error, logger);','QUOTES-NATIVE fetchTickersForSymbols',False),('logger message selection','Value::String(error.message)','Value::String("flattened".into())','QUOTES-NATIVE fetchMarkets',False)]
controls.append(('reporter metadata flatten', 'Value::String(error.message)\n    })', 'Value::String(error.message)\n    }).map_err(|error| ExchangeError::new(error.message))', 'QUOTES-ERROR original metadata fetchMarkets', False))
controls.append(('swallow ticker mapping reporter','report(error.to_exchange_error(), logger)?;\n                return Ok(Some(store.tickers().to_vec()));','let _ = report(error.to_exchange_error(), logger);\n                return Ok(Some(store.tickers().to_vec()));','QUOTES-CONSUMED fetchTickersForSymbols logger',False))
for name,old,new,assertion,bypass_source in controls[args.start_at:]:
 if args.only and name!=args.only:continue
 original=source.read_bytes();test_original=test.read_bytes();s=original.decode();assert s.count(old)==1,(name,s.count(old))
 if args.check_anchors:print(f'{name}: one production anchor');continue
 selected_cmd=cmd.copy()
 if name=='swallow ticker mapping reporter':selected_cmd[-3]='quotes_consumed_fields_match_source'
 try:
  source.write_text(s.replace(old,new))
  if bypass_source:
   t=test_original.decode();line='    assert_source_census(&corpus, SOURCE_CENSUS);';assert t.count(line)==1;t=t.replace(line,'');test.write_text(t)
  r=subprocess.run(selected_cmd,cwd=crate.parent,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: compiled; exit101; {assertion}',flush=True)
 finally:source.write_bytes(original);test.write_bytes(test_original)
 r=subprocess.run(selected_cmd,cwd=crate.parent,env=env,capture_output=True,text=True)
 assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)
if not args.only and not args.check_anchors:
 # Mapping failures must remain inside the source catch.
 original=source.read_bytes();s=original.decode()
 old='report(error.to_exchange_error(), logger)?;\n                Ok(store.markets().to_vec())'
 assert s.count(old)==1
 precedence_cmd=cmd.copy();precedence_cmd[-3]='quotes_market_error_precedence'
 try:
  source.write_text(s.replace(old,'let _ = logger; Err(error.to_exchange_error())'))
  r=subprocess.run(precedence_cmd,cwd=crate.parent,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and 'QUOTES-PRECEDENCE fallback before later malformed row' in out and 'test result: FAILED' in out,(r.returncode,out)
  print('market catch propagation: compiled; exit101; QUOTES-PRECEDENCE fallback before later malformed row',flush=True)
 finally:source.write_bytes(original)
 r=subprocess.run(precedence_cmd,cwd=crate.parent,env=env,capture_output=True,text=True)
 assert r.returncode==0,(r.returncode,r.stdout+r.stderr)
 print('market catch propagation: restored PASS',flush=True)
