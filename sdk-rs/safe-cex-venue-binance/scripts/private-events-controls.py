#!/usr/bin/env python3
"""Serial compiling native/source controls with exact finally restoration."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);a=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',a.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip()==base,'candidate must be direct fresh child of requested head'
rs=crate/'src/private_events.rs';spot_types=Path('packages/safe-cex/src/exchanges/binance/binancespot.types.ts')
inputs=[str(rs),str(crate/'src/private_event_value.rs'),str(crate/'src/lib.rs'),str(crate/'tests/private_events.rs'),str(crate/'tests/fixtures/private-events.json'),str(crate/'scripts/private-events.mjs'),str(crate/'scripts/private-events-controls.py')]
for stem in ['binance','binancepm','binancespot']:
 inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.ws-private.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.ws-private.ts.toml']
inputs += ['packages/safe-cex/src/types.ts','packages/safe-cex/src/utils/createPositionId.ts','packages/safe-cex/src/utils/get-key.ts']
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-private-events')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','private_events','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path)));print(label,result.returncode,flush=True);assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'error[E' not in out and 'test result: FAILED.' in out,'compile error is not a named kill'
 return records[-1]
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('cumulative-remainder',rs,'("remaining", number(amount - filled)),','("remaining", number(amount - filled + 1.0)),','order_events',['native futures regular case1 partial: native cumulative remainder','native pm regular case1 partial: native cumulative remainder','native spot regular case1 partial: native cumulative remainder','3.5 vs expected 2.5']),
 ('algo-order-id',rs,'data.get("aid")?.optional_id("data.aid")','data.get("caid")?.optional_id("data.aid")','futures_algo_events',['native futures algo case0 new: native order id','left: "wrong-client"','right: "501"']),
 ('conditional-filled-default',rs,'fallback(filled, number(0.0))','fallback(filled, number(1.0))','pm_conditional_events',['native pm conditional case7 falsy-filled: native filled','1.0 vs expected 0.0']),
 ('futures-position-notional',rs,'number((signed * entry + upnl).abs())','number((signed * entry).abs())','futures_account_events',['native futures account case0 normal: native notional','30.75 vs expected 34.35']),
 ('pm-position-info-identity',rs,'("info", position.clone()),','("info", p.clone()),','pm_account_events',['native pm account case0 normal: native info original position identity']),
 ('spot-balance-total',rs,'let balance = record([\n            ("free", number(free)),\n            ("used", number(used)),\n            ("total", number(total)),','let balance = record([\n            ("free", number(free)),\n            ("used", number(used)),\n            ("total", number(total + 1.0)),','spot_account_events',['native spot account case0 normal: native aggregate total','51.0 vs expected 50.0']),
 ('same-spelled-enum-string',rs,'EventValue::Enum(PrivateEnum::Status(OrderStatus::Open)),','string("open"),','',['native futures regular case0 new: native order status: native enum representation','native pm regular case0 new: native order status: native enum representation','native spot regular case0 new: native order status: native enum representation','native futures algo case0 new: native order status: native enum representation','native pm conditional case0 new: native order status: native enum representation']),
 ('spot-actual-source-type',spot_types,'export const ORDER_TYPE_SPOT: Record<string, OrderType> = {\n  LIMIT: OrderType.Limit,','export const ORDER_TYPE_SPOT: Record<string, OrderType> = {\n  LIMIT: OrderType.Market,','spot_order_events',['source spot regular case0 "new"','String("market")','String("limit")']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 source=originals[str(path)].decode();assert source.count(before)==1,(label,source.count(before))
 try:
  path.write_text(source.replace(before,after));record=run(f'{iteration}-{label}',test+([selector] if selector else [])+['--','--include-ignored'],101,named)
  record['mutation']=dict(path=str(path),before=before,after=after,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed after all26 native Spot regular cases at actual-source assertion' if path==spot_types else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'src/private_event_value.rs'),str(crate/'tests/private_events.rs')])
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
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]];assert len(changes)==8 and all(r['before']=='pending' and r['after']=='ported' for r in changes),'exact8 existing pending->ported only; no backwards rows'
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='162 authored venue cases (3x26regular,16algo,15conditional,2x18account,17spot-account); stopped regularcase25missing-event/algocase15empty/conditionalcase14missing-event/accountcase17empty/spotcase16empty. Eight compiling controls stopped iteration8 Spot actual enum-table corruption; no selected survivor, no completeness claim.',scope='Eight private event bodies at configured current bound Store/emitter/clock/reject/price/scheduler calls. Native f64 and actual typed enums checked directly; mutable Rc graphs retain info/selector/nested identities, undefined and stock inherited table values. forEach snapshots length and current entries; find visits initial slots; Spot for-of live length and Object.entries ordered snapshots. Fresh Spot dictionaries retain stock __proto__ setter and own-data-shadow behavior. Existing PositionFetchState.pending_events Cell can be supplied directly. Acyclic decoded JSON and these observed native graph mutations, including self-array builtin ID coercion. Class/Store/transport/scheduler composition, arbitrary custom callables/getters/iterators/prototype chains, symbol-key properties, non-BMP string-object-spread/lone UTF16 error text and broad cyclic-graph coercion are outside this boundary.',not_run=['full Binance suite (only new private_events target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/Store/transport/scheduler composition','arbitrary custom-callable/getter/iterator/prototype behavior','non-BMP string-object-spread/lone UTF16 error text','Infinity/signed-zero output fixture proof','exhaustive case/mutation enumeration','whole venue audit'])
(crate/'PRIVATE_EVENTS_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
