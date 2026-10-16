#!/usr/bin/env python3
"""Serial, SHA-bound nine-row message entry gates. Every mutation is restored."""
import argparse,hashlib,json,os,platform,subprocess,tomllib
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--gate-sha',required=True);parser.add_argument('--base-sha',required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[3];os.chdir(root);crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse',args.gate_sha],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-message-entry')/sha;logs.mkdir(parents=True,exist_ok=True)
inputs=[crate/'src'/name for name in ['public_message.rs','spot_public_message.rs','private_message.rs','public_candle_delegate.rs','lib.rs']]+[crate/'Cargo.toml',crate/'tests/message_entry.rs',crate/'scripts/message-entry.mjs',Path(__file__).relative_to(root),Path('packages/safe-cex/src/exchanges/base.ws.ts'),Path('packages/json-parse/src/jsonParse.ts')]
files=['binance.ws-public.ts','binancepm.ws-public.ts','binancespot.ws-public.ts','binance.ws-private.ts','binancepm.ws-private.ts']
for file in files:inputs += [Path('packages/safe-cex/src/exchanges/binance')/file,Path('sdk-rs/parity/manifest/exchanges/binance')/(file+'.toml')]
originals={p:p.read_bytes() for p in inputs}
for p,b in originals.items():assert b==subprocess.check_output(['git','show',f'{sha}:{p}']),p
records=[]
def run(label,cmd,expected=0,assertion=None):
 p=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_bytes(p.stdout);output=p.stdout.decode(errors='replace')
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=p.returncode,output=output,output_sha256=hashlib.sha256(p.stdout).hexdigest()))
 print(label,'exit',p.returncode,flush=True)
 assert p.returncode==expected,output
 if assertion:
  assert 'Finished `test` profile' in output and assertion in output and 'test result: FAILED. 0 passed; 1 failed;' in output,output
  assert 'left:' in output and 'right:' in output,output
  records[-1]['named_failure']=assertion
 elif label in ['baseline','final-restore']:
  assert 'test result: ok. 9 passed; 0 failed; 0 ignored;' in output,output
 elif label=='extractor-check':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output,output
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','message_entry','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
controls=[
 ('futures_market_message','public_message.rs','on_futures_market_message','dispatch(data)','{ let _ = (dispatch, data); Ok(()) }','dispatch count','0 versus1'),
 ('futures_public_message','public_message.rs','on_futures_public_message','dispatch(data)','{ let _ = (dispatch, data); Ok(()) }','dispatch count','0 versus1'),
 ('pm_market_message','public_message.rs','on_pm_market_message','dispatch(data)','{ let _ = (dispatch, data); Ok(()) }','dispatch count','0 versus1'),
 ('pm_public_message','public_message.rs','on_pm_public_message','dispatch(data)','{ let _ = (dispatch, data); Ok(()) }','dispatch count','0 versus1'),
 ('spot_public_message','spot_public_message.rs','on_spot_public_message','dispatch(data)','{ let _ = (dispatch, data); Ok(()) }','dispatch count','0 versus1'),
 ('futures_private_message','private_message.rs','on_futures_private_message','"ACCOUNT_UPDATE"','"ACCOUNT_UPDATE_CORRUPT"','callback count','0 versus1'),
 ('pm_private_message','private_message.rs','on_pm_private_message','"ACCOUNT_UPDATE"','"ACCOUNT_UPDATE_CORRUPT"','callback count','0 versus1'),
 ('futures_candle_delegate','public_candle_delegate.rs','listen_futures_public_ohlcv','    listen_market(opts, callback)','    opts.borrow_mut().limit = Some(8.0);\n    listen_market(opts, callback)','options limit','8 versus7'),
 ('pm_candle_delegate','public_candle_delegate.rs','listen_pm_public_ohlcv','    listen_market(opts, callback)','    opts.borrow_mut().limit = Some(8.0);\n    listen_market(opts, callback)','options limit','8 versus7'),
]
for lane,file,function,before,after,assertion,pair in controls:
 source=crate/'src'/file;original=originals[source].decode();start=original.index('pub fn '+function);end=original.find('\n// ts:',start)
 if end<0:end=len(original)
 body=original[start:end];assert body.count(before)==1
 try:
  source.write_text(original[:start]+body.replace(before,after)+original[end:])
  run(lane+'-native-control',cargo+['--','--include-ignored',lane,'--exact'],101,f'MESSAGE {lane} native {assertion} case0')
  records[-1].update(call_site=str(source)+'::'+function,mutation_before=before,mutation_after=after,value_pair=pair)
 finally:source.write_bytes(originals[source])
 assert source.read_bytes()==originals[source]
# Live-source corruption proves the actual TS bodies are re-extracted on each run.
for lane,file,klass,member,before,after,assertion,pair in [
 ('futures_private_message','binance.ws-private.ts','BinancePrivateWebsocket','onMessage',"'ACCOUNT_UPDATE'","'ACCOUNT_UPDATE_CORRUPT'",'MESSAGE futures_private_message source case0/trace array count','native6 versus source4'),
 ('pm_candle_delegate','binancepm.ws-public.ts','BinancePMPublicWebsocket','listenOHLCV','    return this.marketWebsocket.listenOHLCV(opts, callback);','    opts.limit = 8;\n    return this.marketWebsocket.listenOHLCV(opts, callback);','MESSAGE pm_candle_delegate source case0/opts/limit decoded number','native7 versus source8'),
]:
 source=Path('packages/safe-cex/src/exchanges/binance')/file;original=originals[source].decode();class_start=original.index('export class '+klass+' ');start=original.index('  '+member+' =',class_start);end=original.index('\n  };',start)+len('\n  };');body=original[start:end];assert body.count(before)==1
 try:
  source.write_text(original[:start]+body.replace(before,after)+original[end:])
  run(lane+'-source-control',cargo+['--','--include-ignored',lane,'--exact'],101,assertion)
  records[-1].update(call_site=str(source)+'::'+klass+'.'+member,mutation_before=before,mutation_after=after,value_pair=pair,native_assertions='All native cases for this lane completed before this source comparison.')
 finally:source.write_bytes(originals[source])
 assert source.read_bytes()==originals[source]
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',*[str(p) for p in inputs if p.suffix=='.rs' and p.name!='lib.rs']])
run('git-diff-check',['git','diff','--check'])
for p,b in originals.items():assert p.read_bytes()==b,p
subprocess.run(['git','diff','--exit-code',sha,'--',*[str(p) for p in inputs]],check=True)
def census(revision):
 rows={}
 for p in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for row in tomllib.loads(subprocess.check_output(['git','show',f'{revision}:{p}'],text=True))['symbol']:
   key=(str(p),row['ts'],row['kind']);assert key not in rows;rows[key]=row['status']
 return rows
before,after=census(base),census(sha);assert before.keys()==after.keys()
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes)==9 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
preliminary=[]
for label,path in [('initial','/tmp/binance-message-preliminary.log'),('fixed','/tmp/binance-message-preliminary-fixed.log')]:
 if Path(path).exists():preliminary.append(dict(label=label,gate_sha=None,exit=101 if label=='initial' else 0,output=Path(path).read_text(),claim='uncommitted exploratory run, not a candidate SHA gate'))
evidence=dict(gate_sha=sha,parent_built_on=base,unreviewed_work_in_base=False,moved_pending_to_ported=9,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,records=records,restored_inputs_sha256={str(p):hashlib.sha256(b).hexdigest() for p,b in originals.items()},survivors=[],enumeration_floor='154 authored cases: five public message methods each14 stopping preinitialized, two private methods each36 stopping preinitialized, two OHLCV delegates each6 stopping repeat-cleanup. Controls stopped iteration11 at PM actual-source delegate case0 options.limit7 versus8; nine compiling native controls then two actual-source controls. No exhaustive input/mutation or whole-venue claim.',scope='Shared BaseWebSocket health/statistics errors cross a by-value ExchangeError API: values/name/message/metadata are preserved, native object identity is not claimed. Configured read/dispatch/diagnostic/handler/delegate Rc errors preserve referent identity. The wire failure category original for health errors means matching original error value, whereas the source checks its own original error referent. Nine synchronous extracted bodies: configured MessageEvent.data accessor (including undefined), shared production BaseWebSocket disposal/traffic tracking and private ordinary-topic statistics, actual shared JSON parser, configured public dispatch/diagnostic/handler/pong/companion-market methods. Data accessor and bound methods/receivers are configured class boundaries; concrete classes and shared public dispatch remain pending. Captured MessageData cells are stable: primitive cells immutable; object/array cells retain value/kind and identity, with property/element mutation only. Replacing a captured cell value/kind is outside this configured boundary. Decoded stock JSON values and callback property mutations are supported; executable property hooks, arbitrary own-undefined/non-JSON values remain outside this data domain. Fake timer jobs retained but not pumped; no runtime timer cancellation/execution ownership proof.',completed_pre_documentation_proof=dict(gate_sha='277451975db8872eed8346888e82d764b062e827',controls_named_failed=11,restored_tests_exit=0,scoped_check_exit=0,extractor_exit=0,reason='Passed before captured-cell preconditions were documented. Final records below are freshly measured, not retagged.',progress_output=Path('/tmp/binance-message-proof-completed-277.log').read_text()),failed_proof_attempt=dict(gate_sha='b5230d8864b2bee28c917eddcff86b13dcbd7eb1',baseline_exit=0,baseline_passed=9,first_control_exit=101,runner_exit=1,control_killed=False,reason='Deleting dispatch left unused arguments under -D warnings; compile failure is not a kill. Input restored by finally; corrected sink mutation consumes arguments without calling dispatcher.',output=Path('/tmp/binance-message-proof-attempt1.log').read_text()),preliminary_runs=preliminary,additional_preliminary_retry=dict(exit=101,gate_sha=None,named_assertion='MESSAGE pm_private_message source case17/failure',reason='Attempted correction script failed before editing; an unchanged preliminary test ran again. Output was subsequently overwritten by the corrected exploratory pass; this is not a candidate gate.'),manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output=Path('/tmp/binance-message-writer.log').read_text()),environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),not_run=['full Binance crate test suite (only new message_entry target)','workspace compilation','host clippy','wasm clippy','CI','concrete class composition/shared public dispatch','background timer execution/cancellation ownership','replacing captured MessageData cells or mutating primitive cells','arbitrary executable property hooks/non-JSON own undefined payload fields','whole venue audit','cross-engine verification'])
(crate/'MESSAGE_ENTRY_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n');print('Evidence recorded at',sha,flush=True)
