#!/usr/bin/env python3
"""Serial compiling native/source controls with exact finally restoration."""
import argparse,hashlib,json,os,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base-sha',required=True);a=p.parse_args()
crate=Path('sdk-rs/safe-cex-venue-binance');sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=subprocess.check_output(['git','rev-parse',a.base_sha],text=True).strip()
parent=subprocess.check_output(['git','rev-parse',sha+'^'],text=True).strip();
source_parent=parent
while source_parent!=base and subprocess.check_output(['git','rev-parse',source_parent+'^'],text=True).strip()!=base:
 assert subprocess.check_output(['git','diff','--name-only',source_parent+'^',source_parent],text=True).splitlines()==[str(crate/'scripts/lifecycle-controls.py')],'only runner correction allowed after fresh source child'
 source_parent=subprocess.check_output(['git','rev-parse',source_parent+'^'],text=True).strip()
if parent!=base:
 assert subprocess.check_output(['git','rev-parse',source_parent+'^'],text=True).strip()==base
 assert subprocess.check_output(['git','diff','--name-only',parent,sha],text=True).splitlines()==[str(crate/'scripts/lifecycle-controls.py')]

rs=crate/'src/lifecycle.rs';source_path=Path('packages/safe-cex/src/exchanges/binance/binance.exchange.ts')
inputs=[str(rs),str(crate/'src/lib.rs'),str(crate/'src/private_event_value.rs'),str(crate/'src/request_weight.rs'),str(crate/'tests/lifecycle.rs'),str(crate/'tests/fixtures/lifecycle.json'),str(crate/'scripts/lifecycle.mjs'),str(crate/'scripts/lifecycle-controls.py')]
for stem in ['binance','binancepm','binancespot']:inputs += [f'packages/safe-cex/src/exchanges/binance/{stem}.exchange.ts',f'sdk-rs/parity/manifest/exchanges/binance/{stem}.exchange.ts.toml']
inputs += ['sdk-rs/safe-cex-num/src/lib.rs','sdk-rs/safe-cex-exchange/src/runtime.rs','sdk-rs/Cargo.toml','sdk-rs/Cargo.lock',str(crate/'Cargo.toml')]
originals={s:Path(s).read_bytes() for s in inputs}
for s,data in originals.items():assert subprocess.check_output(['git','show',sha+':'+s])==data,s
records=[];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');logs=Path('sdk-rs/target/binance-lifecycle')/sha;logs.mkdir(parents=True,exist_ok=True)
test=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','lifecycle','--jobs','2']
def run(label,cmd,expected=0,assertions=()):
 result=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=result.stdout.decode(errors='replace');path=logs/(label+'.log');path.write_bytes(result.stdout)
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=result.returncode,output=out,output_sha256=hashlib.sha256(result.stdout).hexdigest(),raw_log=str(path)));print(label,result.returncode,flush=True);assert result.returncode==expected,out
 for assertion in assertions:assert assertion in out,(assertion,out)
 if expected==101:assert 'error[E' not in out and 'test result: FAILED.' in out,'compile error is not a named kill'
 return records[-1]
run('baseline',test+['--','--include-ignored'],assertions=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
controls=[
 ('startup-markets-loaded',rs,'let flags = loaded(b, &["markets"])?;','let flags = loaded(b, &["markets"])?; flags.define("markets", EventValue::Bool(false));','start',['native futures start case0 normal: native markets loaded','native pm start case0 normal: native markets loaded','native spot start case0 normal: native markets loaded','left: false','right: true']),
 ('private-start-orders',rs,'record([("orders", orders), ("loaded", flags)])','record([("orders", { let _ = &orders; EventValue::Array(Default::default()) }), ("loaded", flags)])','private_start',['native futures private case0 normal: native original orders reference','native pm private case0 normal: native original orders reference','native spot private case0 normal: native original orders reference']),
 ('setter-after-await',rs,'let setter = b.capture(LifecycleSync::SetSetting)?;\n        let mode = call(b, LifecycleAsync::PositionMode).await?;','let mode = call(b, LifecycleAsync::PositionMode).await?;\n        let setter = b.capture(LifecycleSync::SetSetting)?;','private_start',['native futures private case10 setter-getter-fail: native mode fetch count','native pm private case10 setter-getter-fail: native mode fetch count','left: 1','right: 0']),
 ('futures-risk-boundary',rs,'const RISK_INTERVAL: f64 = 75000.0;','const RISK_INTERVAL: f64 = 75001.0;','futures_tick',['native futures tick case30 risk-due: native risk request count','left: 0','right: 1']),
 ('pm-next-count',rs,'vec![plus_one(count)?]','vec![{ let _ = plus_one(count)?; number(2.0) }]','pm_tick',['native pm tick case0 normal: native next tick count','2.0 vs expected 1.0']),
 ('spot-loop-interval',rs,'schedule(b, None, interval)?;','let _ = &interval; schedule(b, None, number(1.0))?;','spot_tick',['native spot tick case0 normal: native timer interval','1.0 vs expected 7200.0']),
 ('actual-source-health-interval',source_path,'const PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS = 3_000;','const PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS = 4_000;','futures_tick',['source futures tick case0 "normal"','Number(4000.0)','Number(3000.0)']),
]
for iteration,(label,path,before,after,selector,named) in enumerate(controls,1):
 source=originals[str(path)].decode();assert source.count(before)==1,(label,source.count(before))
 try:
  path.write_text(source.replace(before,after));record=run(f'{iteration}-{label}',test+([selector] if selector else [])+['--','--include-ignored'],101,named)
  record['mutation']=dict(path=str(path),before=before,after=after,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),named_assertions=named,disposition='killed after all36 native Futures tick cases at actual-source assertion' if path==source_path else 'killed at decoded native assertion')
 finally:path.write_bytes(originals[str(path)])
run('final-restore',test+['--','--include-ignored'],assertions=('test result: ok. 9 passed; 0 failed; 0 ignored;',))
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'],assertions=('Would update 0 manifests; 0 added, 0 orphaned symbols.',))
run('scoped-rustfmt',['/opt/homebrew/opt/rustup/bin/rustfmt','--edition','2024','--check',str(rs),str(crate/'src/private_event_value.rs'),str(crate/'tests/lifecycle.rs')])
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
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]];assert len(changes)==9 and all(r['before']=='pending' and r['after']=='ported' for r in changes),'exact9 existing pending->ported only; no backwards rows'
counts=lambda r:{s:list(r.values()).count(s) for s in ['ported','divergent','pending']}
d=dict(gate_sha=sha,parent_built_on=parent,feature_base=base,records=records,restored_inputs_sha256={s:hashlib.sha256(data).hexdigest() for s,data in originals.items()},moved_pending_to_ported=9,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,enumeration_floor='228 authored venue cases from76 inputs (3x22start,3x18private-start,3x36tick). Stopped startcase21missing-length/privatecase17mode-missing/tickcase35degraded-before. Seven compiling controls stopped iteration7 actual Futures source health interval. No selected survivor; no completeness claim.',scope='Nine startup/private-start/tick bodies at current bound getters/methods/Store/socket/fetch/clock/imported-loop operations. Already-launched owner-retained async jobs driven through settlement; concrete class/Store/socket/imported-loop runtime wiring and ignored async rejection/drop/unwind/sleep/RAF ownership pending. Immutable Error Rc/name/message/code/response and current caller hooks are preserved; mutable exception metadata/non-Error rejections unclaimed. Native shared f64/undefined graphs retain original payload/nested references and shallow loaded copies. Same-owner timer resolver is acquired non-observably and reads the current tick callee/receiver at fire before PM count+1 coercion. Dense decoded data and observed field/property mutations supported; arbitrary executable graph hooks/custom prototypes/iterators/symbol keys, non-BMP string spreads/lone UTF16 and broad coercion/prototype completeness unclaimed. Paired exact due/before fixtures cover Futures risk75s, account25s and degraded3s; only risk+1 threshold mutation selected/killed, no account/degraded +/-1 mutation proof.', not_run=['full Binance suite (only new lifecycle target)','workspace compilation','host clippy','wasm clippy','CI','concrete class/Store/socket/loop runtime wiring','discarded async rejection/drop/unwind/sleep/RAF ownership','mutable exception metadata/non-Error rejection values','arbitrary executable graph/custom prototype/iterator/symbol-key behavior','non-BMP string spreads/lone UTF16','Infinity/signed-zero output fixture proof','account/degraded threshold +/-1 mutation proof (paired due/before fixtures do run)','exhaustive case/mutation enumeration','whole venue audit'])
(crate/'LIFECYCLE_GATE_EXITS.json').write_text(json.dumps(d,indent=2)+'\n');print('Gate sheet',sha,'9pending->ported,0->divergent',flush=True)
