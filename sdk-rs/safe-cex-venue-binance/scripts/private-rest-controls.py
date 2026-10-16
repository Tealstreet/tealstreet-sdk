#!/usr/bin/env python3
"""Serial compiling native/source controls with exact finally restoration."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);a=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',a.base_sha],text=True).strip()
parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip()
if parent!=base:
 assert subprocess.check_output(['git','rev-parse',parent+'^'],text=True).strip()==base,'only one reviewed source commit may precede runner-only fix'
 assert subprocess.check_output(['git','diff','--name-only',parent,sha],text=True).splitlines()==['sdk-rs/safe-cex-venue-binance/scripts/private-rest-controls.py'],'runner correction only; source parent stays fresh'
assert subprocess.run(['git','merge-base','--is-ancestor',base,sha]).returncode==0
rs=crate/'src/private_rest.rs';source_types=Path('packages/safe-cex/src/exchanges/binance/binancepm.types.ts')
inputs=[str(rs),str(crate/'src/lib.rs'),str(crate/'tests/private_rest.rs'),str(crate/'tests/fixtures/private-rest.json'),str(crate/'scripts/private-rest.mjs'),str(crate/'scripts/private-rest-controls.py')]
for stem in ['binance','binancepm','binancespot']:
 inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml']
inputs += ['packages/safe-cex/src/types.ts','packages/logging/src/LogSeverity.ts','packages/safe-cex/src/utils/get-key.ts','packages/safe-cex/src/utils/safe-math.ts','packages/safe-cex/src/exchanges/binance/binance.order-history.ts','packages/safe-cex/src/exchanges/clientOrderId.ts']
inputs += [str(crate/'src'/(name+'.rs')) for name in ['private_event_value','order_history','response_shape','response_message']]
inputs += ['sdk-rs/safe-cex-exchange/src/client_order_id.rs','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-num/src/get_key.rs','sdk-rs/safe-cex-num/src/json_stringify.rs',str(crate/'Cargo.toml')]
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-private-rest')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','private_rest','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path)));print(label,result.returncode,flush=True);assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'error[E' not in out and 'test result: FAILED.' in out,'compile error is not a named kill'
 return records[-1]
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('regular-remainder',rs,'let remaining = subtract(amount_raw, filled_raw)?;','let remaining = subtract(amount_raw, filled_raw)? + 1.0;','order_snapshots',['native futures orders case0 normal: native remainder','native pm regular case0 normal: native remainder','native pm conditional case0 normal: native remainder','native spot orders case0 normal: native remainder','1.07 vs expected 0.07']),
 ('futures-algo-id',rs,'let id = aid.optional_id("algoId")?;','let id = keyed(&o, "clientAlgoId")?.optional_id("algoId")?;','futures_order_snapshots',['native futures orders case0 normal: native algo id','left: "wrong-client"','right: "501"']),
 ('pm-combined-order',rs,'let out = concat(first, second)?;','let out = concat(second, first)?;','pm_combined_order_snapshots',['native pm combined case0 normal: native first order id','left: "conditional"','right: "regular"']),
 ('pm-used-balance',rs,'used += item_total - item_free;','used += item_total + item_free;','pm_balance_snapshots',['native pm balance case0 normal: native used','22.75 vs expected 10.25']),
 ('spot-total-balance',rs,'("total", number(total)),\n                    ("free", number(free)),\n                    ("used", number(used)),\n                    ("upnl", number(0.0)),','("total", number(total + 1.0)),\n                    ("free", number(free)),\n                    ("used", number(used)),\n                    ("upnl", number(0.0)),','spot_balance_snapshots',['native spot balance case0 normal: native total','34.0 vs expected 33.0']),
 ('lookup-remaining',rs,'difference.max(0.0)','difference.max(0.0) + 1.0','futures_client_order_lookup',['native futures lookup case0 normal: native remaining','1.07 vs expected 0.07']),
 ('aggregate-error-order',rs,'(Some(a), Some(b)) => a.reaction_order <= b.reaction_order,','(Some(_), Some(_)) => true,','futures_order_snapshots',['native futures orders case23 earlier-algo-reject: native catch log message','left: "fetchOrders error: snapshot fixture"','right: "fetchOrders error: algo first"']),
 ('pm-actual-source-balance-endpoint',source_types,"BALANCE: '/papi/v1/balance',","BALANCE: '/papi/v1/balance-mutation',",'pm_balance_snapshots',['source pm balance case0 "normal"','/papi/v1/balance-mutation','/papi/v1/balance']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 source=originals[str(path)].decode();assert source.count(before)==1,(label,source.count(before))
 try:
  path.write_text(source.replace(before,after));record=run(f'{iteration}-{label}',test+([selector] if selector else [])+['--','--include-ignored'],101,named)
  record['mutation']=dict(path=str(path),before=before,after=after,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed after all14 native PM balance cases at actual-source assertion' if path==source_types else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'src/private_event_value.rs'),str(crate/'tests/private_rest.rs')])
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
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='152 authored venue cases from66 inputs (4x24orders,12combined,2x14balance,16lookup); stopped orderscase23earlier-algo-reject/combinedcase11string-iterables/balancecase13proto-asset/lookupcase15orders-fail. Eight compiling controls stopped iteration8 PM actual source endpoint corruption; no selected survivor, no completeness claim.',scope='Eight private REST snapshot and client-ID lookup bodies at configured current HTTP/Store/notice/clock/price/imported-helper operations. Native f64 and actual enum variants checked directly; Rc native graphs retain original row/response/fallback/info referents and shallow algo metadata. Source raw safe-math precision and optional toString error paths, initial-length mapping, live Spot balance iterator, stock dictionary prototype setter, stale flags and effect ordering preserved. Request futures return decoded response.data and owning runtime aggregate reaction order; all settled reactions must poll Ready, unique orders are for this Promise.all attachment (not historical settlement). Request owner retains background completion if await abandoned. Concrete class/client/runtime/Store/imported-helper composition, response/data getter timing, mutable exception metadata and nested error response/message referent identities, source array deletion/truncation creating map holes, custom callables/getters/iterators/prototypes/symbol keys and lone UTF16 remain outside this boundary. Authored native adapters genuinely call existing history/client-ID Rust helpers in mapped/closed-enum domain; existing historical helper inherited-output refusal is unchanged and unclaimed.', not_run=['full Binance suite (only new private_rest target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/runtime/Store/imported-helper composition','response-object/data getter identity/timing','mutable exception metadata or nested error response/message identity','non-Error rejection values','array deletion/truncation creating map holes','custom-callable/getter/iterator/prototype/symbol-key behavior','lone UTF16 error text','inherited-output historical helper completeness','Infinity/signed-zero output fixture proof','exhaustive case/mutation enumeration','whole venue audit'])
(crate/'PRIVATE_REST_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
