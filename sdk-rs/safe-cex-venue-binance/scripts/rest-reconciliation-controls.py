#!/usr/bin/env python3
"""Serial SHA-bound gates: eight new-row controls, an error-identity control, two source controls."""
import argparse,hashlib,json,os,platform,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--gate-sha',required=True);p.add_argument('--base-sha',required=True);args=p.parse_args()
root=Path(__file__).resolve().parents[3];os.chdir(root);crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse',args.gate_sha],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha;subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-rest-reconciliation')/sha;logs.mkdir(parents=True,exist_ok=True)
inputs=[crate/'src'/n for n in ['request_weight.rs','exchange_error.rs','orders_retry.rs','account_leverage_overlay.rs','lib.rs','response_shape.rs','response_message.rs','reconciliation.rs']]+[crate/'Cargo.toml',crate/'tests/rest_reconciliation.rs',crate/'scripts/rest-reconciliation.mjs',Path(__file__).relative_to(root),Path('packages/safe-cex/src/utils/get-key.ts')]
for venue in ['binance','binancepm']:inputs += [Path('packages/safe-cex/src/exchanges/binance')/(venue+'.exchange.ts'),Path('packages/safe-cex/src/exchanges/binance')/(venue+'.types.ts'),Path('sdk-rs/parity/manifest/exchanges/binance')/(venue+'.exchange.ts.toml')]
originals={path:path.read_bytes() for path in inputs}
for path,data in originals.items():assert data==subprocess.check_output(['git','show',f'{sha}:{path}']),path
records=[]
def run(label,cmd,expected=0,assertion=None):
 r=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=r.stdout.decode(errors='replace');(logs/(label+'.log')).write_bytes(r.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=r.returncode,output=out,output_sha256=hashlib.sha256(r.stdout).hexdigest()));print(label,'exit',r.returncode,flush=True)
 assert r.returncode==expected,out
 if assertion:
  assert 'Finished `test` profile' in out and assertion in out and 'test result: FAILED. 0 passed; 1 failed;' in out,out
  records[-1]['named_failure']=assertion
 elif label in ['baseline','final-restore']:assert 'test result: ok. 8 passed; 0 failed; 0 ignored;' in out,out
 elif label=='extractor-check':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in out,out
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','rest_reconciliation','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
controls=[
 ('response_header','request_weight.rs','get_response_header','pub fn ','property(&name.to_ascii_lowercase())','property(&name.to_ascii_uppercase())','REST header native result case0','4.2 versus3.6'),
 ('response_weight','request_weight.rs','observe_request_weight','pub fn ','retry * 1_000.0','retry * 1_000.0 + 1.0','REST weight native duration case0','2501 versus2500'),
 ('weight_interceptors','request_weight.rs','install_request_weight_guards','pub fn ','Rc::ptr_eq(seen, client)','Rc::ptr_eq(seen, client) && false','REST install native register count case0','4 versus2'),
 ('exchange_error','exchange_error.rs','emit_binance_exchange_error','pub fn ','"Exchange Error: {}"','"Exchange Error: CORRUPT {}"','REST emit native prefix case0','Exchange Error: CORRUPT venue versusExchange Error: venue'),
 ('futures_orders_retry','orders_retry.rs','retry_futures_orders_snapshot','pub async fn ','15_000.0','15_001.0','REST retry_futures native fetch count case0','0 versus1'),
 ('pm_orders_retry','orders_retry.rs','retry_pm_orders_snapshot','pub async fn ','15_000.0','15_001.0','REST retry_pm native fetch count case0','0 versus1'),
 ('futures_leverage_overlay','account_leverage_overlay.rs','build_overlay','fn ','maxima.insert(symbol.to_owned(), max)','maxima.insert(symbol.to_owned(), max + 1.0)','REST overlay_futures native max case0 row0','126 versus125'),
 ('pm_leverage_overlay','account_leverage_overlay.rs','build_overlay','fn ','maxima.insert(symbol.to_owned(), max)','maxima.insert(symbol.to_owned(), max + 1.0)','REST overlay_pm native max case0 row0','126 versus125'),
 ('weight_interceptors','request_weight.rs','install_request_weight_guards','pub fn ','register(&client, fulfilled, rejected)?;','register(&client, fulfilled, rejected).map_err(|error| Rc::new((*error).clone()))?;','REST install registration native original error identity','fresh Rc versus original Rc, with equal decoded error values'),
]
for iteration,(test,file,function,decl,before,after,assertion,pair) in enumerate(controls,1):
 path=crate/'src'/file;original=originals[path].decode();start=original.index(decl+function);end=original.find('\n// ts:',start)
 if end<0:end=len(original)
 body=original[start:end];assert body.count(before)==1,(function,before)
 try:
  path.write_text(original[:start]+body.replace(before,after)+original[end:]);run(str(iteration)+'-'+test+'-native-control',cargo+['--','--include-ignored',test,'--exact'],101,assertion)
  records[-1].update(call_site=str(path)+'::'+function,mutation_before=before,mutation_after=after,value_pair=pair)
 finally:path.write_bytes(originals[path])
 assert path.read_bytes()==originals[path]
# Actual source dependency corruption: oracle reads the real imported endpoint object.
path=Path('packages/safe-cex/src/exchanges/binance/binance.types.ts');original=originals[path].decode();before="LEVERAGE_BRACKET: '/fapi/v1/leverageBracket'";after="LEVERAGE_BRACKET: '/fapi/v1/leverageBracket-CORRUPT'";assert original.count(before)==1
try:
 path.write_text(original.replace(before,after));run('10-futures-overlay-source-endpoint-control',cargo+['--','--include-ignored','futures_leverage_overlay','--exact'],101,'REST overlay_futures source case0/endpoint')
 records[-1].update(call_site=str(path)+'::ENDPOINTS.LEVERAGE_BRACKET',mutation_before=before,mutation_after=after,value_pair='/fapi/v1/leverageBracket versus/fapi/v1/leverageBracket-CORRUPT',native_assertions='All nineteen native Futures overlay cases completed before source comparison.')
finally:path.write_bytes(originals[path])
path=Path('packages/safe-cex/src/exchanges/binance/binancepm.exchange.ts');original=originals[path].decode();start=original.index('  private retryOrdersSnapshot =');end=original.index('\n  };',start)+len('\n  };');body=original[start:end];before='< 15_000';after='< 15_001';assert body.count(before)==1
try:
 path.write_text(original[:start]+body.replace(before,after)+original[end:]);run('11-pm-retry-source-threshold-control',cargo+['--','--include-ignored','pm_orders_retry','--exact'],101,'REST retry_pm source case0/trace array count')
 records[-1].update(call_site=str(path)+'::BinancePMExchange#retryOrdersSnapshot',mutation_before=before,mutation_after=after,value_pair='native trace7 versus source trace1',native_assertions='All eighteen native PM retry cases completed before source comparison.')
finally:path.write_bytes(originals[path])
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',*[str(crate/'src'/n) for n in ['request_weight.rs','exchange_error.rs','orders_retry.rs','account_leverage_overlay.rs']],str(crate/'tests/rest_reconciliation.rs')])
run('git-diff-check',['git','diff','--check'])
run('exact-input-restoration',['git','diff','--exit-code',sha,'--',*[str(p) for p in inputs]])
for path,data in originals.items():assert path.read_bytes()==data,path
def census(rev):
 result={}
 for path in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for row in tomllib.loads(subprocess.check_output(['git','show',f'{rev}:{path}'],text=True))['symbol']:
   k=(str(path),row['ts'],row['kind']);assert k not in result;result[k]=row['status']
 return result
b,a=census(base),census(sha);assert b.keys()==a.keys();changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=b[k],after=a[k]) for k in b if b[k]!=a[k]]
assert len(changes)==8 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
count=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
preliminary=[]
for name,exit,path in [('compile-fail',101,'/tmp/binance-rest-preliminary-compile-fail.log'),('fixture-fail',101,'/tmp/binance-rest-preliminary-fixture-fail.log'),('before-review',0,'/tmp/binance-rest-preliminary-before-review.log'),('reviewed',0,'/tmp/binance-rest-preliminary-reviewed.log'),('strengthened',0,'/tmp/binance-rest-preliminary-strengthened.log')]:
 preliminary.append(dict(label=name,exit=exit,gate_sha=None,output=Path(path).read_text(),claim='uncommitted exploratory run, not a gate at the candidate SHA'))
evidence=dict(gate_sha=sha,gate_immediate_parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip(),parent_built_on=base,unreviewed_external_work_in_base=False,moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=count(b),census_after=count(a),changed_rows=changes,records=records,restored_inputs_sha256={str(p):hashlib.sha256(data).hexdigest() for p,data in originals.items()},survivors=[],preliminary_runs=preliminary,scope='Eight extracted REST/reconciliation bodies at configured header/HTTP/current-method/Store/overlay-cache/emission bindings. ASCII header names; normalized header truthiness; decoded JSON status/header values; typed numeric/bool retry state and five native StoreDataLoaded flags; original generic orders handle; Error-instance discriminator + consistent decoded properties and reference-carrying original opaque Error token. Non-Error owned Value message fields preserve values, not object referent identity. Dense decoded bracket rows and string-symbol typed markets; reference-carrying LeverageMarket adapter supplies shallow copies, production Market value adapter preserves declared DTO fields without nested-pointer topology claim. No concrete class/client composition, executable state/error/response hooks, own undefined/non-JSON fields, extra/nonboolean loaded properties, non-string market symbols or lone UTF16 code-unit error text in malformed string brackets; future/Promise scheduling and cancellation/drop ownership remain pending.',enumeration_floor='133 authored cases: header15 stopped property-fail; weight21 stopped pause-fail; installer11 stopped error-getter-changes-method; emission12 stopped emit-fail; retry2x18 stopped loaded-disposes; overlay2x19 stopped overlay-repeat. Nine compiling native controls plus two actual-source dependency/body controls, stopped iteration11 at PM retry sourcecase0. No exhaustive case/mutation or whole-venue claim.',review_proof_gaps_addressed=['Direct native original failure Rc/name/message/code assertions before projection, including interceptor paths.','Direct native generated error names/messages before any source projection.','Oracle evaluates actual imported source ENDPOINTS objects, not fixture endpoint literals.','Equal-valued distinct clients + finite retry-second multiplication overflow fixtures; original Error handles vs non-Error Value message identity boundary explicit.'],manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output=Path('/tmp/binance-rest-writer.log').read_text()),environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),not_run=['full Binance crate suite (only new rest_reconciliation target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/client/Store/overlay-cache composition','Promise/future scheduling, cancellation/drop ownership','non-Error object-message referent identity','owned Market DTO nested referent topology','Unicode non-ASCII header transforms','lone UTF16 malformed-bracket error text','arbitrary hooks/own undefined/non-JSON data','whole venue audit','cross-engine verification'])
(crate/'REST_RECONCILIATION_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n');print('Evidence recorded at',sha,'8 pending->ported,0->divergent',flush=True)
