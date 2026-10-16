#!/usr/bin/env python3
"""Serial new-row controls: compiling named native/source failures, exact restore."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);args=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==base,'source candidate must be direct fresh child of requested integration'
rs=crate/'src/order_operations.rs';source=Path('packages/safe-cex/src/exchanges/binance/binancespot.exchange.ts')
inputs=[str(crate/'src'/p) for p in ['order_operations.rs','lib.rs','private_event_value.rs','request_weight.rs','order_logic.rs']]
inputs += [str(crate/'tests/order_operations.rs'),str(crate/'tests/fixtures/order-operations.json'),str(crate/'scripts/order-operations.mjs'),str(crate/'scripts/order-operations-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml',str(crate/'src/generated'/f'{stem}.rs')]
inputs += ['packages/safe-cex/src/types.ts','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-types/src/object_map.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==31 and len(set(inputs))==31
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-order-operations')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','order_operations','--jobs','2']
def run(label,cmd,expected=0,named=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 r=dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path));records.append(r);(logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,result.returncode,flush=True)
 assert result.returncode==expected,out
 for assertion in named:assert assertion in out,(assertion,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E' not in out,'compile failure is INVALID, not a kill'
 return r
run('baseline',test+['--','--include-ignored'],named=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
controls=[
 ('futures-cancel-chunk-size',rs,'values.chunks(10)','values.chunks(9)','futures_cancel',['native futures cancel case3 chunk-eleven request0: native orderIdList','9007199254740999']),
 ('pm-cancel-conditional-endpoint',rs,'"CONDITIONAL_ORDER"','"ORDER"','pm_cancel',['native pm cancel case0 normal request2: native endpoint','left: "/papi/v1/um/order"','right: "/papi/v1/um/conditional/order"']),
 ('spot-cancel-captured-id',rs,'("origClientOrderId", id),','("origClientOrderId", string(format!("mutated-{}", id.text()?))),','spot_cancel',['native spot cancel case0 normal request0: native origClientOrderId','left: "mutated-9007199254740993"','right: "9007199254740993"']),
 ('update-overrides',rs,'new.define(key, changes.get(key)?);','new.define(key, order.get(key)?);','update',[f'native {v} update case0 normal: native new price' for v in ['futures','pm','spot']]+['left: 12.3','right: 12.4']),
 ('futures-place-algo-result',rs,'spread_ids(&mut out, batch(algo)?.await?)?;','let _ids = batch(algo)?.await?;','futures_place',['native futures place case0 normal: native result count: native array count','left: 2','right: 3']),
 ('pm-spot-place-batch-argument',rs,'batch(payloads)?.await','batch(array(vec![payloads]))?.await','place',[f'native {v} place case0 normal: native original batch payload identity' for v in ['pm','spot']]+['left: Some(false)','right: Some(true)']),
 ('actual-source-spot-price-override',source,"if ('price' in update) newOrder.price = update.price;","if ('price' in update) newOrder.price = order.price;",'spot_update',['source spot update case0']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==1,(label,text.count(before))
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named)
  r['mutation']=dict(path=str(path),before=before,after=after,replacements=1,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at source case0 after all11 Spot update native cases' if path==source else 'killed at decoded native assertion')
  if path==source:
   line=next(s for s in r['output'].splitlines() if s.startswith('source spot update case0: native '));left,right=line.split(': native ',1)[1].split(' source ',1);native,oracle=json.loads(left),json.loads(right);assert native['forwarded']['price']==12.4 and oracle['forwarded']['price']==12.3;r['mutation']['decoded_source_witness']={'native_forwarded_price':native['forwarded']['price'],'source_forwarded_price':oracle['forwarded']['price']}
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],named=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],named=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/order_operations.rs')])
run('git-diff-check',['git','diff','--check'])
run('exact-input-restoration',['git','diff','--exit-code',sha,'--',*inputs])
for s,data in originals.items():assert Path(s).read_bytes()==data,s
def rows(rev):
 result={}
 for path in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for r in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(path)],text=True))['symbol']:
   key=(str(path),r['ts'],r['kind']);assert key not in result;result[key]=r['status']
 return result
before,after=rows(base),rows(sha);assert before.keys()==after.keys();changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]];assert len(changes)==9 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=9,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='120 authored cases:17cancel/11update/12place each of Futures/PM/Spot. Stopped cancel16null-input/update10null-update/place11batch-capture-mutates. Seven compiling controls stopped iteration7 actual-source Spot price override after all11 Spot update native cases. No selected survivors; no exhaustive claim.',scope='Nine extracted cancel/update/single-place bodies at configured current bound client/Store/format/batch/cancel/place/conflict/ID/classifier/notice operations. Canonical native enums, f64/own undefined/null/raw flags, original order/options/payload/result/error Rc, late/captured IDs, exact generated TypeError values and independent counts precede source comparisons. Actual source oracle executes method bodies, actual Futures classifier and installed lodash/groupBy/chunk/p-iteration helpers; broad helper completeness/event-loop scheduling unclaimed. No shared SDK type/trait change. Dense arrays/nullish order collections; batch helpers return dense arrays or ASCII strings for Futures spread. Concrete class/client/Store/helper wiring and started request drop/unwind ownership pending. Executable properties/custom iterators/sparse arrays/arbitrary malformed helper outputs/mutable or non-Error exceptions/nested Error response/message referent identity/non-BMP/lone UTF16 coercion excluded.',not_run=['full Binance suite (only new order_operations target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/Store/helper runtime wiring','event-loop/p-iteration scheduling completeness','started request drop/unwind/ignored rejection ownership','nested Error response/message referent identity','executable properties/custom iterators/sparse arrays/arbitrary malformed helper outputs','mutable Error/non-Error exceptions','non-BMP/lone UTF16/full coercion','broad imported helper completeness','exhaustive mutation/case enumeration','whole venue audit'])
(crate/'ORDER_OPERATIONS_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'9pending->ported,0->divergent',flush=True)
