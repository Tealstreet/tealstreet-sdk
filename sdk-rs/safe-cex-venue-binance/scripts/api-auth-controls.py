#!/usr/bin/env python3
"""Serial compiling controls, native failures, source corruption and exact restore."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);a=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',a.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==base,'candidate must be direct fresh child of requested integration'
rs=crate/'src/api_factory.rs';source_path=Path('packages/safe-cex/src/exchanges/binance/binance.api.ts')
inputs=[str(crate/'src'/p) for p in ['api_factory.rs','lib.rs','private_event_value.rs','request_weight.rs','spot_protocol.rs','signing.rs']]
inputs += [str(crate/'tests/api_auth.rs'),str(crate/'tests/fixtures/api-auth.json'),str(crate/'scripts/api-auth.mjs'),str(crate/'scripts/api-auth-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.api.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.api.ts.toml',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',str(crate/'src/generated'/f'{stem}.rs')]
inputs += ['packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts','sdk-rs/parity/manifest/exchanges/binance/binancespot.ws-private.ts.toml','packages/safe-cex/src/utils/retryReadsOnly.ts','packages/safe-cex/src/remoteSigningAdapter.ts','sdk-rs/safe-cex-crypto/src/lib.rs','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==34 and len(set(inputs))==34
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-api-auth')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','api_auth','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path)));print(label,result.returncode,flush=True);assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E' not in out,'compile failure is not a kill'
 return records[-1]
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('factory-config-timeout',rs,'let timeout = recv(&options, window)?;','let timeout = number(recv(&options, window)?.number()? + 1.0);',1,'',[f'native {f} factory case0 normal: native timeout' for f in ['futures','futures_public','pm','papi','fapi','pm_public','spot']]+['left: 20001.0','right: 20000.0']),
 ('futures-silent-error',rs,'[-2011.0, -2013.0, -4059.0, -5022.0]','[-2012.0, -2013.0, -4059.0, -5022.0]',1,'futures',['native futures response case1 silent success: native emitted message count','left: 1','right: 0']),
 ('spot-auth-recv-window',rs,'("apiKey", api_key),\n        ("recvWindow", number(s::USER_DATA_RECV_WINDOW)),','("apiKey", api_key),\n        ("recvWindow", number(s::USER_DATA_RECV_WINDOW + 1.0)),',1,'auth',['native Spot auth case0 local: native recvWindow','left: 5001.0','right: 5000.0']),
 ('actual-source-retry-policy',source_path,'networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY))','networkErrorOnRead()',2,'futures',['source futures factory case0','"probes":[true,true,true,true,false,true,false]','"probes":[true,true,false,false,false,false,false]']),
]
for iteration,(label,path,before,after,occurrences,selector,named) in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==occurrences,(label,text.count(before))
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+([selector] if selector else [])+['--','--include-ignored'],101,named)
  r['mutation']=dict(path=str(path),before=before,after=after,replacements=occurrences,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed after all12 Futures factory native cases and their authored callback vectors at source case0' if path==source_path else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/api_auth.rs')])
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
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]];assert len(changes)==8 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='97 authored case floor:7x12 factories +13 Spot auth. Stopped factorycase11 recv-string/authcase12 remote-object. Callback vectors10request inputs (stop9skip-fail),9response inputs for both success/error (stop8message-object); seven retry policy probes stop6delete/non-listen. Four compiling controls stopped iteration4 actual-source retry policy. No selected survivors; no exhaustive enumeration claim.',scope='Seven API factory bodies and Spot user-data subscription params builder at configured create/retry/request-use/Header/transport/sign/response-use/clock/remote-sign/HMAC operations. Native ClientMode/ResponseTransform/QuerySerializer/RetryRead/RequestReturn/ApiRejection variants, f64/undefined/shared referents and original Rc Failure metadata are asserted before source comparison. Existing Repeat query codec executed on one independently authored JSON params vector; broad codec/helper completeness unchanged and unclaimed. Concrete Axios/Header/client/retry/network-error classification/transport/signBinanceRequest/remote-signer/socket composition and retained async job ownership pending. Spot reads current parent options after clock, preserves captured API key, captures signer method/receiver before accountName options read, forwards raw signature. Actual UTF8 HMAC adapter exercised against independent Python expected hex. Immutable errors only; executable graph getters/custom prototypes/symbols and Node Buffer/key-object coercion excluded.',not_run=['full Binance suite (only new api_auth target)','workspace compilation','host clippy','wasm clippy','CI','concrete Axios/Header/client/retry/transport/signBinanceRequest/remote-signer/socket runtime wiring','background async job cancellation/drop/unwind/ignored rejection ownership','actual network-error classifier completeness','arbitrary executable graph getters/custom prototypes/symbol keys','mutable Error metadata/remote-sign-helper non-Error rejection values','Node Buffer/key-object HMAC coercion','non-BMP/lone UTF16/full coercion and query-codec completeness','exhaustive mutation/case enumeration','whole venue audit'])
(crate/'API_AUTH_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
