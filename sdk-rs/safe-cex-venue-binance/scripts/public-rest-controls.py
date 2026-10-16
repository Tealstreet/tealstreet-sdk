#!/usr/bin/env python3
"""Serial, compiling controls; exact restore in finally. Run from worktree root."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);a=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',a.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip()==base,'candidate must be fresh child of requested integration head'
rs=crate/'src/public_rest.rs';spot_types=Path('packages/safe-cex/src/exchanges/binance/binancespot.types.ts')
inputs=[str(rs),str(crate/'src/lib.rs'),str(crate/'tests/public_rest.rs'),str(crate/'tests/fixtures/public-rest.json'),str(crate/'scripts/public-rest.mjs'),str(crate/'scripts/public-rest-controls.py')]
for stem in ['binance','binancepm','binancespot']:
 inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml']
inputs += ['packages/safe-cex/src/utils/get-key.ts',str(crate/'src/response_shape.rs'),str(crate/'src/exchange_error.rs'),str(crate/'src/response_message.rs')]
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-public-rest')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','public_rest','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path)))
 print(label,result.returncode,flush=True);assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'error[E' not in out and 'test result: FAILED.' in out,'compile failure is not a mutation kill'
 return records[-1]
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
controls=[
 ('futures-market-precision',rs,'amount: vf(amt, "stepSize")?,','amount: vf(amt, "stepSize")? + 1.0,','futures_markets',['native markets futures case0 normal row0: native amount precision','1.1 vs expected 0.1']),
 ('pm-required-filters',rs,'if venue == RestVenue::PM && (p.is_none() || amt.is_none() || m_amt.is_none()) {','if false && venue == RestVenue::PM && (p.is_none() || amt.is_none() || m_amt.is_none()) {','pm_markets',['native markets pm case3 missing-filters: native market count','left: 1','right: 0']),
 ('spot-market-precision',rs,'amount: if amount_step > 0.0 {\n                amount_step','amount: if amount_step > 0.0 {\n                amount_step + 1.0','spot_markets',['native markets spot case0 normal row0: native amount precision','1.1 vs expected 0.1']),
 ('ticker-bid',rs,'bid: vf(Some(book), "bidPrice")?,','bid: vf(Some(book), "bidPrice")? + 1.0,','tickers',['native tickers futures case0 normal row0: native bid','native tickers pm case0 normal row0: native bid','native tickers spot case0 normal row0: native bid','11.1 vs expected 10.1']),
 ('candle-milliseconds',rs,'timestamp: js_number(values.first()).map_err(|e| type_error(e.message))? / 1000.0,','timestamp: js_number(values.first()).map_err(|e| type_error(e.message))? / 1001.0,','ohlcv',['native ohlcv futures case0 normal row0: native timestamp','native ohlcv pm case0 normal row0: native timestamp','native ohlcv spot case0 normal row0: native timestamp','vs expected 121.0']),
 ('spot-source-endpoint',spot_types,"TICKERS_24H: '/api/v3/ticker/24hr',","TICKERS_24H: '/api/v3/ticker/24hr-CORRUPT',",'spot_tickers',['source tickers spot case0 "normal"','/api/v3/ticker/24hr-CORRUPT']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 source=originals[str(path)].decode();assert source.count(before)==1,(label,source.count(before))
 try:
  path.write_text(source.replace(before,after));record=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named)
  record['mutation']=dict(path=str(path),before=before,after=after,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at named assertion; source endpoint after all15 native Spot ticker cases' if path==spot_types else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/public_rest.rs')])
run('git-diff-check',['git','diff','--check'])
run('exact-input-restoration',['git','diff','--exit-code',sha,'--',*inputs])
for s,data in originals.items():assert Path(s).read_bytes()==data,s
def rows(rev):
 result={}
 for path in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for r in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(path)],text=True))['symbol']:
   key=(str(path),r['ts'],r['kind']);assert key not in result;result[key]=r['status']
 return result
before,after=rows(base),rows(sha);assert before.keys()==after.keys()
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes)==9 and all(r['before']=='pending' and r['after']=='ported' for r in changes),'no backwards rows or divergent additions'
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=9,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='150 authored venue cases (3x17 markets,3x15 tickers,3x18 OHLCV), stopped markets case16 coercion-order/tickers case14 map-null-order/OHLCV case17 subtract-fail. Six compiling controls, stopped iteration6 Spot real source endpoint; no selected survivor, no completeness claim.',scope='Nine extracted public REST bodies over dense decoded JSON/current HTTP and Store bindings. Native RestMarket preserves uncoerced identity/base/quote/leverage and real MarketPrecision/MarketLimitsAmount; actual Ticker/Candle DTOs preserve native special floats. Rc handles preserve Error and Store fallback array identity. Configured stock Dayjs dependency owns time clipping/calendar/DST; native tests exercise UTC adapter and real source Dayjs. String-keyed declared market dictionary records; independent cross-response JSON object graphs, BMP string bracket errors. Own undefined/non-JSON graphs/executable properties/custom collections/non-Error rejections, malformed inherited dictionary entries, lone UTF16 error text, class/client/store/date-library composition and scheduling/drop ownership remain pending.',not_run=['full Binance crate suite (only new public_rest target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/Store/Dayjs composition','calendar/DST implementation outside configured Dayjs binding','future/Promise scheduling or cancellation/drop ownership','whole venue audit','exhaustive case/mutation enumeration'])
(crate/'PUBLIC_REST_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'9pending->ported,0->divergent',flush=True)
