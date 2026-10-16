#!/usr/bin/env python3
"""Serial new-family controls, focused native proof repairs, source corruption/restore."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);args=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==base,'candidate must be direct child of requested fresh integration'
rs=crate/'src/order_batches.rs';source=Path('packages/safe-cex/src/exchanges/binance/binancepm.exchange.ts')
inputs=[str(crate/'src'/p)for p in ['order_batches.rs','order_operations.rs','lib.rs','private_event_value.rs','request_weight.rs','order_logic.rs']]
inputs += [str(crate/'tests/order_batches.rs'),str(crate/'tests/fixtures/order-batches.json'),str(crate/'scripts/order-batches.mjs'),str(crate/'scripts/order-batches-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml',str(crate/'src/generated'/f'{stem}.rs')]
inputs += ['packages/safe-cex/src/types.ts','packages/safe-cex/src/utils/safe-math.ts','packages/math/src/isNotNumber.ts','packages/logging/src/LogSeverity.ts','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-num/src/json_stringify.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-types/src/object_map.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==36 and len(set(inputs))==36
originals={s:Path(s).read_bytes()for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-order-batches')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','order_batches','--jobs','2']
def run(label,cmd,expected=0,named=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout);r=dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path));records.append(r);(logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,result.returncode,flush=True)
 assert result.returncode==expected,out
 for a in named:assert a in out,(a,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E'not in out,'compile failure is INVALID, not a kill'
 return r
run('baseline',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('many-futures-regular-algo-swap',rs,'spread(&mut regular, r)?;\n        spread(&mut algo, a)?;','spread(&mut regular, a)?;\n        spread(&mut algo, r)?;','many_futures_target',['native many_futures case0 normal: native batch client ID0','left: "a-A"','right: "r-A"']),
 ('many-pm-spot-flattening',rs,'payloads.extend(a.borrow().iter().cloned())','payloads.push(array(a.borrow().iter().cloned().collect()))','many',[f'native many_{v} case0 normal: native batch client ID0' for v in ['pm','spot']]+['expected native String','Undefined']),
 ('regular-classifier-error-identity',rs,'post_only(b, Rejection::Error(e.clone()))?','post_only(b, Rejection::Error(Rc::new(e.as_ref().clone())))?','regular',['native regular case4 post-reject: native classifier caught Error identity','left: false','right: true']),
 ('regular-encoded-price',rs,'let encoded = b.stringify(array(lot.to_vec()))?;','let encoded = b.stringify(array(lot.to_vec()))?;\n                let encoded = match encoded {EventValue::String(s)=>string(s.replace("\\\"price\\\":12.3", "\\\"price\\\":12.0")),v=>v};','regular',['native regular case0 normal: native decoded price','left: 12.0','right: 12.3']),
 ('algo-exchange-id',rs,'id(&data, "algoId", "data.algoId")?','id(&data, "orderId", "data.algoId")?','algo',['native algo case0 normal: native ID0','expected native String','9007199254740993','Undefined']),
 ('pm-position-quantity',rs,'fallback(found, number(0.0))','{ let _observed_contracts = found; number(0.0) }','pm',['native pm case0 normal: native fallback quantity3','expected native String','4.2','Number(0.0)']),
 ('spot-late-client-id',rs,'post(b, Venue::Spot, "ORDER", payload.clone()).await?;\n                ids.push(payload.get("newClientOrderId")?);','ids.push(payload.get("newClientOrderId")?);\n                post(b, Venue::Spot, "ORDER", payload.clone()).await?;','spot',['native spot case5 mutate-client-ids: native ID0','left: "c0"','right: "late-1"']),
 ('conflict-adjusted-close-intent',rs,'!opts.get("close")?.truthy() && amount.truthy()','!opts.get("close")?.truthy() && (opts.get("amount")?.truthy() || amount.truthy())','conflict',['native conflict case5 adjust-zero: native orders count','left: 0','right: 1']),
 ('actual-source-pm-zero-quantity',source,'...(+quantity','...(!+quantity','pm',['source pm case0']),
]
for iteration,(label,path,before,after,selector,named)in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==1,(label,text.count(before))
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named);r['mutation']=dict(path=str(path),before=before,after=after,replacements=1,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at source case0 after all16 PM batch native cases'if path==source else'killed at decoded native assertion')
  if path==source:
   line=next(s for s in r['output'].splitlines()if s.startswith('source pm case0: native '));left,right=line.split(': native ',1)[1].split(' source ',1);native,oracle=json.loads(left),json.loads(right);assert native['posts'][3]['payload']['quantity']=='4.2'and oracle['posts'][3]['payload']['quantity']=='0';r['mutation']['decoded_source_witness']={'native_post3_quantity':native['posts'][3]['payload']['quantity'],'source_post3_quantity':oracle['posts'][3]['payload']['quantity']}
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],named=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/order_batches.rs')])
run('git-diff-check',['git','diff','--check']);run('exact-input-restoration',['git','diff','--exit-code',sha,'--',*inputs])
for s,data in originals.items():assert Path(s).read_bytes()==data,s
def rows(rev):
 out={}
 for path in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for r in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(path)],text=True))['symbol']:
   k=(str(path),r['ts'],r['kind']);assert k not in out;out[k]=r['status']
 return out
before,after=rows(base),rows(sha);assert before.keys()==after.keys();changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k])for k in before if before[k]!=after[k]];assert len(changes)==8 and all(r['before']=='pending'and r['after']=='ported'for r in changes)
counts=lambda r:{s:list(r.values()).count(s)for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest()for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,environment={'SAFE_CEX_TYPESCRIPT_PATH':env['SAFE_CEX_TYPESCRIPT_PATH'],'cargo_jobs':2},enumeration_floor='106 authored cases:19regular/10algo/16PM/10Spot/12many per venue/15conflict. Stopped regular18stringify-reject/algo9emit-reject/PM15null-input/Spot9emit-reject/many11null-input/conflict14precision-string. Nine compiling controls stopped iteration9 actual-source PM quantity after all16 PM native cases. No selected survivor/exhaustive claim.',scope='Eight extracted multi-order/batch/conflict bodies at configured current client/Store/assert/formatter/batch/classifier/encoder/adjust/log/emit operations. Native enums/f64/undefined/IDs/raw quantities/own fields/shallow aliases/original Row and caught Error arguments/exact original or generated metadata/independent counts asserted before source comparison. batchOrders decoded once with authored native fields, never re-encoded proof. Actual source methods/classifier/math+guard/chunk helpers executed; broad helper/codecs/event-loop completeness unclaimed. POST decoded response.data, response/data getters unclaimed. Dense arrays and documented nullish chunk inputs, prepared acyclic stock JSON, valid formatter/batch output domain; executable properties/custom iterators/sparse or arbitrary array-like/cyclic/toJSON graphs/malformed helper outputs/mutable or non-Error exceptions/nested error response/message identity/non-BMP/lone UTF16 excluded. Concrete class/client/Store/helper composition and started-request drop/unwind/ignored rejection ownership pending. No shared SDK type/trait change.',not_run=['full Binance suite (only new order_batches target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/Store/helper runtime wiring','event-loop scheduling and broad imported helper/codec completeness','started-request drop/unwind/ignored rejection ownership','response/data getter timing','nested Error response/message referent identity','executable properties/custom iterators/sparse or arbitrary array-like/cyclic/toJSON graphs','arbitrary malformed formatter/batch output domains','mutable Error/non-Error exceptions','non-BMP/lone UTF16/full coercion','exhaustive cases/mutations','whole venue audit'])
(crate/'ORDER_BATCHES_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
