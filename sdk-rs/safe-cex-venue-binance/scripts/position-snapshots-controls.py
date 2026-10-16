#!/usr/bin/env python3
"""Serial position snapshot family controls and SHA-bound restores/gates."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);args=p.parse_args();c=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==base
rs=c/'src/position_snapshots.rs';helper=c/'src/private_rest.rs';ts=Path('packages/safe-cex/src/exchanges/binance/binance.exchange.ts')
inputs=[str(c/'src'/f)for f in ['position_snapshots.rs','private_rest.rs','private_event_value.rs','request_weight.rs','lib.rs','response_shape.rs','response_message.rs','generated/binance.rs','generated/binancepm.rs']]
inputs+=[str(c/'tests/position_snapshots.rs'),str(c/'tests/fixtures/position-snapshots.json'),str(c/'scripts/position-snapshots.mjs'),str(c/'scripts/position-snapshots-controls.py'),str(c/'tests/private_rest.rs'),str(c/'scripts/private-rest.mjs'),str(c/'tests/fixtures/private-rest.json')]
for stem in ['binance','binancepm']:inputs+=[f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts']
inputs+=['packages/safe-cex/src/utils/get-key.ts','packages/safe-cex/src/utils/createPositionId.ts','packages/safe-cex/src/types.ts','packages/logging/src/LogSeverity.ts','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-num/src/get_key.rs','sdk-rs/safe-cex-num/src/table_lookup.rs','sdk-rs/safe-cex-types/src/model_utils.rs','sdk-rs/safe-cex-types/src/js_coercion.rs','sdk-rs/safe-cex-types/src/object_map.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(c/'Cargo.toml'),'package.json','yarn.lock','sdk-rs/parity/manifest/exchanges/binance/binance.exchange.ts.toml','sdk-rs/parity/manifest/exchanges/binance/binancepm.exchange.ts.toml','packages/safe-cex/src/exchanges/binance/binancespot.exchange.ts','packages/safe-cex/src/exchanges/binance/binancespot.types.ts','packages/safe-cex/src/exchanges/binance/binance.order-history.ts','packages/safe-cex/src/exchanges/clientOrderId.ts','packages/safe-cex/src/utils/safe-math.ts','packages/safe-cex/src/utils/uuid.ts']
assert len(inputs)==45 and len(set(inputs))==45;originals={p:Path(p).read_bytes()for p in inputs}
for p,v in originals.items():assert subprocess.check_output(['git','show',sha+':'+p])==v,p
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-position-snapshots')/sha;logs.mkdir(parents=True,exist_ok=True);cargo='/opt/homebrew/opt/rustup/bin/cargo';test=[cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','position_snapshots','--jobs','2']
def run(label,cmd,expected=0,named=()):
 r=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=r.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(r.stdout);record=dict(label=label,gate_sha=sha,command=cmd,exit=r.returncode,output=out,raw_log=str(path),output_sha256=hashlib.sha256(r.stdout).hexdigest());records.append(record);(logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,r.returncode,flush=True);assert r.returncode==expected,out
 for n in named:assert n in out,(n,out)
 if expected==101:assert 'Finished `test` profile'in out and 'test result: FAILED.'in out and 'error[E'not in out,'compile failures INVALID'
 return record
run('baseline',test+['--','--include-ignored'],named=('test result: ok. 3 passed; 0 failed; 0 ignored;',))
controls=[
 ('futures-config-leverage',rs,'let lev = leverage(config)?;','let lev = p.get("leverage")?.float()?;',1,'futures_positions',['native first leverage 6 vs 4.2','native 4.2 vs expected 6.0']),
 ('pm-margin-division',rs,'number(notional / lev)','number(notional * lev)',1,'pm_positions',['native first margin size division']),
 ('balance-direct-price-kind',rs,'price = ticker.get("last")?;','price = number(ticker.get("last")?.number()?);',1,'futures_balance',['native balance string-price total','expected 9.1']),
 ('pm-root-number-shape',helper,'if matches!(v, EventValue::Number(_))','if matches!(v, EventValue::Undefined)',1,'pm_positions',['native pm-infinity exact error message','received null']),
 ('actual-source-balance-usd-total',ts,'total += walletUsdValue;','total += assetWallet;',1,'futures_balance',['actual source Balance balance case0','27.42','15.9']),
]
for i,(label,path,before,after,count,selector,named)in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==count,(label,text.count(before))
 try:
  path.write_text(text.replace(before,after));r=run(f'{i}-{label}',test+[selector,'--','--include-ignored'],101,named);r['mutation']=dict(path=str(path),before=before,after=after,replacements=count,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed by actual source comparison after all26nativebalance cases'if path==ts else'killed by decoded native value/kind/message assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],named=('test result: ok. 3 passed; 0 failed; 0 ignored;',))
run('private-rest-regression',[cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','private_rest','--jobs','2','--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',[cargo,'check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],named=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(helper),str(c/'tests/position_snapshots.rs')]);run('git-diff-check',['git','diff','--check']);run('exact-input-restoration',['git','diff','--exit-code',sha,'--',*inputs])
for p,v in originals.items():assert Path(p).read_bytes()==v,p
def rows(rev):
 r={}
 for p in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for x in tomllib.loads(subprocess.check_output(['git','show',rev+':'+str(p)],text=True))['symbol']:
   k=(str(p),x['ts'],x['kind']);assert k not in r;r[k]=x['status']
 return r
before,after=rows(base),rows(sha);assert before.keys()==after.keys()and len(after)==286;changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k])for k in before if before[k]!=after[k]];assert len(changes)==3 and all(x['before']=='pending'and x['after']=='ported'for x in changes);counts=lambda r:{s:list(r.values()).count(s)for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={p:hashlib.sha256(v).hexdigest()for p,v in originals.items()},moved_pending_to_ported=3,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='74authored newcases:24Fposition +24PMposition +26Fbalance; stoppedposition23proto-config/balance25bool-price. Fivecompiling controls stoppediteration5 actualsourcebalance USDtotal afterall26nativebalance cases. Existingprivate_rest regression8/0/0 only; no whole8target/exhaustive claim.',scope='Three snapshot bodies at current bound HTTP/Store/ID/emitter/log methods. Native EVERYreturned position enums/IDorder/f64 fields/info/configRc, OWNundefined vs missing fields, original fallback/current positions/balance/shallowinfo/assets/nestedRc, native balancef64/NaN/log severity/text/counts/flag/partial effects and immutable ErrorRc/fullmetadata before actual AST method/helper comparisons. NativeString symbols/IDs,dense own-data arrays/records, configured ID/helper/HTTP response and Promise.allreaction-order contracts. RootNumber∞ PMshape corrected withoutlossyJSONkind projection; existingmalformedobject code/msg nonfinite metadata at private_rest::pm_rows remains an inventoried UNREMEASURED finding outside finiteJSONmetadata boundary. Nativebyte/order/API/extractedbody coverage, not wholeclass/Store/importedhelper/socket/task assembly.',not_run=['full Binance suite (newposition_snapshots+affectedprivate_rest targets only)','workspace compilation','host clippy','wasm clippy','CI','concrete class/Store/helper/client assembly','response-object/data getters','sparse/custom iterable/executable properties/full coercion/full UTF16','mutable/non-Error failures/nested error-response message referent identity','malformedresponse object nonfinite code/message metadata atprivate_rest::pm_rows','host scheduling/discarded async ownership/drop/unwind','exhaustive cases/mutations/whole venue audit'])
(c/'POSITION_SNAPSHOTS_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'3pending->ported,0->divergent',flush=True)
