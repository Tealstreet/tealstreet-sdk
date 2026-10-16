#!/usr/bin/env python3
"""Serial scoped listener-family controls, focused proof repairs, source corruption/restore."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);args=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==base,'candidate must be direct child of requested fresh integration'
rs=crate/'src/public_listeners.rs';source=Path('packages/safe-cex/src/exchanges/binance/binancespot.ws-public.ts')
inputs=[str(crate/'src'/f)for f in ['public_listeners.rs','lib.rs','private_event_value.rs','request_weight.rs','orderbook.rs','protocol_types.rs']]
inputs += [str(crate/'tests/public_listeners.rs'),str(crate/'tests/fixtures/public-listeners.json'),str(crate/'scripts/public-listeners.mjs'),str(crate/'scripts/public-listeners-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.ws-public.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.ws-public.ts.toml',str(crate/'src/generated'/f'{stem}.rs')]
inputs += ['packages/safe-cex/src/types.ts','packages/safe-cex/src/exchanges/binance/binance.orderbook.ts','packages/safe-cex/src/utils/orderbook.ts','packages/safe-cex/src/utils/updateOrderBook.ts','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-types/src/object_map.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-store/src/orderbook.rs','sdk-rs/safe-cex-store/src/update_order_book.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==37 and len(set(inputs))==37
originals={s:Path(s).read_bytes()for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-public-listeners')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','public_listeners','--jobs','2']
def run(label,cmd,expected=0,named=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout);r=dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path));records.append(r);(logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,result.returncode,flush=True)
 assert result.returncode==expected,out
 for a in named:assert a in out,(a,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E'not in out,'compile failure is INVALID, not a kill'
 return r
run('baseline',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
lookup='''let empty = self
                .host
                .state()
                .handlers
                .borrow()
                .get(&self.handler)
                .ok_or_else(|| {
                    type_error("Cannot read properties of undefined (reading 'length')")
                })?
                .is_empty();'''
controls=[
 ('candle-native-timestamp',rs,'timestamp: k.get("t")?.number()? / 1000.0','timestamp: k.get("t")?.number()? / 100.0',1,'candles_',['native candles Futures normal: native timestamp','left: 12345.67','right: 1234.567']),
 ('native-subscription-method',rs,'string(method)','string(if method == "SUBSCRIBE" {"UNSUBSCRIBE"} else {"SUBSCRIBE"})',1,'_',['native candles Futures normal: native method SUBSCRIBE','String("UNSUBSCRIBE")','native books Futures normal: native method SUBSCRIBE']),
 ('constructor-super-first',rs,'let base = super_and_fields(parent.clone())?;\n    let market_websocket = market(parent)?;','let market_websocket = market(parent.clone())?;\n    let base = super_and_fields(parent)?;',2,'constructors_',['native constructor call order','left: ["market", "super"]','right: ["super", "market"]']),
 ('depth-previous-final-id',rs,'venue != Venue::Spot','venue == Venue::Spot',2,'books_',['native books Futures normal: native previous-final flag','left: false','right: true']),
 ('sync-get-finally-timing',rs,'self.fetching.set(false);\n            self.host.spawn(Box::pin(async move { Err(error) }));','self.fetching.set(true);\n            self.host.spawn(Box::pin(async move { Err(error) }));',1,'books_',['native books Futures get-sync: native same-turn fetch count','left: 1','right: 2']),
 ('late-handler-missing-error',rs,lookup,'let empty = self.host.state().handlers.borrow().get(&self.handler).is_some_and(Vec::is_empty);',1,'books_',['native books Futures reset-removes-handler: native generated error count','left: 0','right: 1']),
 ('actual-source-spot-candle-time',source,'timestamp: json.k.t / 1000','timestamp: json.k.t / 100',1,'candles_spot',['source candles_spot case0']),
]
for iteration,(label,path,before,after,count,selector,named)in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==count,(label,text.count(before),count)
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named);r['mutation']=dict(path=str(path),before=before,after=after,replacements=count,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at decoded source value comparison after all14 Spot native cases'if path==source else'killed at decoded native value/reference/metadata/count assertion')
  if path==source:
   line=next(s for s in r['output'].splitlines()if s.startswith('source candles_spot case0: native '));left,right=line.split(': native ',1)[1].split(' source ',1);native,oracle=json.loads(left),json.loads(right);assert native['candles'][0]['timestamp']==1234.567 and oracle['candles'][0]['timestamp']==12345.67;r['mutation']['decoded_source_witness']={'native_candle0_timestamp':native['candles'][0]['timestamp'],'source_candle0_timestamp':oracle['candles'][0]['timestamp']}
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],named=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/public_listeners.rs')])
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
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest()for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,environment={'SAFE_CEX_TYPESCRIPT_PATH':env['SAFE_CEX_TYPESCRIPT_PATH'],'cargo_jobs':2},enumeration_floor='102 authored cases:14candles per venue/18books per venue/3constructors inFutures+PM. Stopped candles13wrong-events/books17reset-removes-disconnected/constructors2market-error. Seven compiling controls stopped iteration7 actual-source Spot candle timestamp after all14 Spot native cases. No selected survivor/exhaustive claim.',scope='Eight extracted candle/depth listener and explicit companion constructor bodies at current bound timer/send/HTTP/book-helper/super-and-derived-fields/companion operations. Native Candle/Book f64 fields, native request Venue enum/booleans/numeric IDs/method strings, original inner/update element/shared Book/parent/base/companion/Error Rc, exact Error metadata and independently authored counts asserted before source comparisons. Actual source AST method/constructor bodies plus actual orderbook/update/total helpers execute; no output roundtrip proof. Dense ARRAY envelopes, ASCII instrument strings, data properties, numeric topic counts, non-prototype IDs and authored valid snapshot/level payloads. Helper buffer ARRAY-container/Book level-array and level-row alias identity outside supplied helper boundary. POST/GET already-decoded data, response/data getters unclaimed. Concrete class/BaseWebSocket/dispatch/implicit derived field/real companion constructor wiring, host scheduling and discarded async rejection/task ownership/drop/unwind pending. No shared SDK type/trait change.',not_run=['full Binance suite (only new public_listeners target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/BaseWebSocket/dispatch/implicit field/real companion wiring','host scheduling/discarded async job ownership/drop/unwind','response/data getters','buffer ARRAY-container/Book level-array and row alias identity','non-array iterable envelopes/executable properties/sparse arrays/prototype inner IDs/nonnumeric topic counters','arbitrary malformed snapshot/level payloads/non-BMP/lone UTF16/full coercion','mutable/non-Error exceptions','exhaustive cases/mutations','whole venue audit'])
(crate/'PUBLIC_LISTENERS_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
