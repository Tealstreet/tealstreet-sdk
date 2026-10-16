#!/usr/bin/env python3
"""Serial row-family controls: compiled named failures and exact restoration."""
import argparse, hashlib, json, os, subprocess, tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);args=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip()
assert parent==base,'source candidate must be direct child of requested integration'
rs=crate/'src/rest_history.rs';source=Path('packages/safe-cex/src/exchanges/binance/binance.exchange.ts')
inputs=[str(crate/'src'/p) for p in ['rest_history.rs','lib.rs','private_event_value.rs','request_weight.rs','order_history.rs','utils.rs']]
inputs += [str(crate/'tests/rest_history.rs'),str(crate/'tests/fixtures/rest-history.json'),str(crate/'scripts/rest-history.mjs'),str(crate/'scripts/rest-history-controls.py')]
inputs += [f'packages/safe-cex/src/exchanges/binance/{p}' for p in ['binance.exchange.ts','binance.utils.ts','binance.types.ts','binance.order-history.ts']]
inputs += ['packages/safe-cex/src/types.ts','packages/logging/src/LogSeverity.ts']
inputs += [f'packages/safe-cex/src/utils/{p}.ts' for p in ['splitTimeRange','normalizeExecutionPositionSide','incomeCategoryFromType','omitUndefined']]
inputs += ['packages/safe-cex/src/exchanges/income-paging.ts']
inputs += [f'sdk-rs/parity/manifest/exchanges/binance/{p}.toml' for p in ['binance.exchange.ts','binance.utils.ts']]
inputs += [str(crate/'src/generated/binance.rs'),'sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-types/src/model_utils.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/safe-cex-exchange/src/income_paging.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==34 and len(set(inputs))==34
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-rest-history')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','rest_history','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 r=dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path));records.append(r)
 (logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,result.returncode,flush=True)
 assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E' not in out,'compile failure is INVALID, not a kill'
 return r
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('mapper-number-coercion',rs,'out.define("price", number(entry.get("price")?.number()?));','out.define("price", number(entry.get("price")?.float()?));','mapper',['native mapper case1 numeric-prefix: native price','expected native NaN']),
 ('fills-timestamp-unit',rs,'entry.get("time")?.number()? / 1000.0','entry.get("time")?.number()? / 1.0','fills',['native fills case0 normal row0: native timestamp','left: 1700000000123.0','right: 1700000000.0']),
 ('orders-descending-sort',rs,'if !(delta > 0.0) {','if !(delta < 0.0) {','orders',['native orders case0 normal row0: native ID','left: "100"','right: "200"']),
 ('income-native-amount',rs,'out.define("income", number(r.get("income")?.float()?));','out.define("income", number(r.get("income")?.float()? + 1.0));','income',['native page case0 normal row0: native income','native income case0 normal row0: native income','left: 2.25','right: 1.25']),
 ('range-page-callee',rs,'b.page_resolver(IncomePageMethod::Rethrowing)','b.page_resolver(IncomePageMethod::Swallowing)','range',['native range case0 normal: native page method','left: Some(Swallowing)','right: Some(Rethrowing)']),
 ('public-trade-side',rs,'if r.get("isBuyerMaker")?.truthy() {\n                    OrderSide::Sell','if r.get("isBuyerMaker")?.truthy() {\n                    OrderSide::Buy','public_trades_target',['native public_trades case0 normal row0: native side','left: Side(Buy)','right: Side(Sell)']),
 ('seed-limit',rs,'params.define("limit", number(1.0));','params.define("limit", number(2.0));','seed',['native seed case0 normal: native seed limit','left: 2.0','right: 1.0']),
 ('actual-source-range-delay',source,'const INCOME_HISTORY_CHUNK_DELAY_MS = 2_000;','const INCOME_HISTORY_CHUNK_DELAY_MS = 3_000;','range',['source range case0','"chunkDelayMs":2000','"chunkDelayMs":3000']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==1,(label,text.count(before))
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named)
  r['mutation']=dict(path=str(path),before=before,after=after,replacements=1,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at source case0 after all12 range native cases' if path==source else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'src/private_event_value.rs'),str(crate/'tests/rest_history.rs')])
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
assert len(changes)==8 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='91 authored cases: mapper8 stop7null-entry; fills12 stop11log-fail; orders10 stop9emit-fail; page12 and income12 stop11category-appends; range12 stop11log-fail; public trades10 stop9noncall-id; seed15 stop14cleanup-reject. Eight compiling controls stopped iteration8 actual-source delay, after all12 range native cases. No selected survivors; no exhaustive claim.',scope='Eight extracted method/helper bodies at configured bound current GET, decoded response.data, locale/helper/notice/pager/date and retained seed task operations. Native canonical enum references, f64/NaN, own undefined, original records/candle/options/callback/error Rc, error metadata and authored counts asserted before source comparisons. Existing historical-order helper called with native input adaptation and native output assertions; no output roundtrip, broad existing helper refusal/completeness unchanged. Local graph enum extension only; no shared SDK type/trait change. GET envelope/data getter timing and nested immutable Error response/message identity unclaimed. Dense input graphs with observed replacement/append callbacks; custom executable properties/iterators/prototypes/symbols, map holes, mutable Error/non-Error rejections and non-BMP/lone UTF16/full locale coercion excluded. Concrete class/client/socket/helper/pager/date runtime wiring and Promise attachment/drop/unwind/ignored rejection ownership pending.',not_run=['full Binance suite (only new rest_history target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/socket/helper/pager/date runtime wiring','Promise attachment/drop/unwind/ignored rejection ownership','response envelope/data getter timing','nested Error response/message referent identity','custom executable properties/iterators/prototypes/symbols/map holes','mutable Error/non-Error rejections','non-BMP/lone UTF16/full locale coercion','broad existing helper refusal/completeness','exhaustive cases/mutations','whole venue audit'])
(crate/'REST_HISTORY_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
