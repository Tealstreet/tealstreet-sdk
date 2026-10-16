#!/usr/bin/env python3
"""Serial private-session family controls. Compiling failures, exact restores, SHA-bound exits."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);p.add_argument('--candidate-parent');args=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip();parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();assert parent==subprocess.check_output(['git','rev-parse',args.candidate_parent or args.base_sha],text=True).strip(),'candidate parent must match declared build parent';subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
rs=crate/'src/private_session.rs';source=Path('packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts')
inputs=[str(crate/'src'/f)for f in ['private_session.rs','lib.rs','private_event_value.rs','request_weight.rs','spot_protocol.rs']]
inputs += [str(crate/'tests/private_session.rs'),str(crate/'tests/fixtures/private-session.json'),str(crate/'scripts/private-session.mjs'),str(crate/'scripts/private-session-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.ws-private.ts',f'packages/safe-cex/src/exchanges/binance/{stem}.types.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.ws-private.ts.toml',str(crate/'src/generated'/f'{stem}.rs')]
inputs += ['packages/safe-cex/src/types.ts','packages/json-parse/src/jsonParse.ts','packages/safe-cex/src/utils/createPositionId.ts','packages/logging/src/LogSeverity.ts','sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-types/src/object_map.rs','sdk-rs/safe-cex-types/src/lib.rs','sdk-rs/safe-cex-types/src/model_utils.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml'),'package.json','yarn.lock']
assert len(inputs)==41 and len(set(inputs))==41
originals={s:Path(s).read_bytes()for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-private-session')/sha;logs.mkdir(parents=True,exist_ok=True)
cargo='/opt/homebrew/opt/rustup/bin/cargo';test=[cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','private_session','--jobs','2']
def run(label,cmd,expected=0,named=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout);r=dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path));records.append(r);(logs/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(label,result.returncode,flush=True)
 assert result.returncode==expected,out
 for a in named:assert a in out,(a,out)
 if expected==101:assert 'Finished `test` profile' in out and 'test result: FAILED.' in out and 'error[E'not in out,'compile failure is INVALID, not a kill'
 return r
run('baseline',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
controls=[
 ('constructor-primary-role',rs,'owner.assign("_primaryXhr", primary.clone());','owner.assign("_primaryXhr", secondary.clone());',1,'constructors_',['native constructor Futures normal _primaryXhr: original role identity','native constructor PM normal _primaryXhr: original role identity']),
 ('constructor-rate-max',rs,'r.rate_limit(xhr, 3.0)','r.rate_limit(xhr, 4.0)',1,'constructors_',['native constructor rate maxRPS: 3 vs mutation 4','left: 4.0','right: 3.0']),
 ('legacy-retry-factor',rs,'(delay * 2.0).min(60000.0)','(delay * 3.0).min(60000.0)',1,'connect_',['native retry backoff: 10000 vs mutation 15000','left: 15000.0','right: 10000.0']),
 ('spot-credential-predicate',rs,'!key.truthy() || (!secret.truthy() && !remote.truthy())','key.truthy() || (!secret.truthy() && !remote.truthy())',1,'connect_spot_cases',['native connect Spot normal: socket decision','left: false','right: true']),
 ('spot-json-depth-floor',rs,'            d.disable_recursion_limit();','            // Deliberate default-depth ceiling control.',1,'spot_message_cases',['native Spot message deep: route sequence','left: [Track, Diagnostic]','right: [Track, Diagnostic, Stats, Order]']),
 ('confirmed-native-mode',rs,'        PositionMode::Hedged','        PositionMode::OneWay',1,'confirmed_mode_cases',['native confirmed position mode enum','left: OneWay','right: Hedged']),
 ('constructor-late-client-read',rs,'                primary,','                split.get("1")?,',1,'constructors_futures',['native constructor captured guard role 0 tuple-mutates']),
 ('actual-source-spot-status',source,'json.status !== 200 || typeof json.result?.subscriptionId','json.status === 200 || typeof json.result?.subscriptionId',1,'spot_message_cases',['actual source comparison message_spot case 3','"Ping"','"ForceClose"']),
]
for iteration,(label,path,before,after,count,selector,named)in enumerate(controls,1):
 text=originals[str(path)].decode();assert text.count(before)==count,(label,text.count(before),count)
 try:
  path.write_text(text.replace(before,after));r=run(f'{iteration}-{label}',test+[selector,'--','--include-ignored'],101,named);r['mutation']=dict(path=str(path),before=before,after=after,replacements=count,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed at actual source comparison after all15 native Spot message cases'if path==source else'killed at native value/enum/identity/count/route assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],named=('test result: ok. 8 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',[cargo,'check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],named=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'tests/private_session.rs')])
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
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest()for s,data in originals.items()},moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,environment={'SAFE_CEX_TYPESCRIPT_PATH':env['SAFE_CEX_TYPESCRIPT_PATH'],'cargo_jobs':2},enumeration_floor='93 authored cases:10constructors per venue +16legacy connects inFutures/PM +8Spot connects +15Spot messages +8confirmed-mode. Stopped constructors9tuple-mutates/legacy15retry-disposed/Spotconnect7emit-error/message14deep(depth150)/mode7id-error. Eight compiling controls stopped iteration8 actual-source Spot status after all15 native Spot message cases. No survivor/exhaustive claim.',scope='Eight extracted constructor/private-connect/Spot-message/confirmed-mode bodies at configured current operations. Native f64 fields (including actual routed positive Infinity and negative zero), typed Venue/Factory/PositionMode/PositionSide/LogSeverity, exact URLs/auth/error messages, original owner/client/socket/event/info/active-position/Error Rc, full authored Error metadata and independent counts before actual source comparisons. Numeric overflow subscriptionId witness is type/admission-only; exact native Infinity asserted on separately routed deep message. Actual TypeScript AST bodies, actual jsonParse/createPositionId/LogSeverity/types/URL constants execute. No output roundtrip proof. Dense own-data client tuples/arrays, String listen keys/market/position symbols, immutable Error exceptions. New venue-local bindings only; no shared SDK type/trait change; local serde_json feature removes default depth128 ceiling, depth150 is the proved floor, parser resource exhaustion unclaimed.',not_run=['full Binance suite (only new private_session target)','workspace compilation','host clippy','wasm clippy','CI','concrete classes/implicit derived fields/BaseExchange/BaseWebSocket/real child/imported factory/socket composition','concrete Store/worker/subscriber composition','host scheduling/discarded async rejection/task ownership/drop/unwind','non-Error/mutable exceptions/getters/custom iterators/sparse arrays','malformed non-String symbols/non-data metadata/full coercion/non-BMP/lone UTF16/resource exhaustion','exhaustive cases/mutations','whole venue audit'])
(crate/'PRIVATE_SESSION_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'8pending->ported,0->divergent',flush=True)
