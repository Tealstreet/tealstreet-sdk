#!/usr/bin/env python3
"""Serial SHA-bound gates, four native controls and two source controls."""
import argparse,hashlib,json,os,platform,subprocess,tomllib
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--gate-sha',required=True);parser.add_argument('--base-sha',required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[3];os.chdir(root);crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse',args.gate_sha],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-position-refresh')/sha;logs.mkdir(parents=True,exist_ok=True)
inputs=[crate/'src/position_refresh.rs',crate/'src/lib.rs',crate/'tests/position_refresh.rs',crate/'scripts/position-refresh.mjs',Path(__file__).relative_to(root)]
for venue in ['binance','binancepm']:inputs += [Path('sdk-rs/parity/manifest/exchanges/binance')/(venue+'.ws-private.ts.toml'),Path('packages/safe-cex/src/exchanges/binance')/(venue+'.ws-private.ts')]
originals={p:p.read_bytes() for p in inputs}
for p,content in originals.items():assert content==subprocess.check_output(['git','show',f'{sha}:{p}']),p
records=[]
def run(label,cmd,expected=0,assertion=None):
 p=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_bytes(p.stdout);output=p.stdout.decode(errors='replace')
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=p.returncode,output=output,log_sha256=hashlib.sha256(p.stdout).hexdigest()));print(label,'exit',p.returncode,flush=True)
 assert p.returncode==expected,output
 if assertion:
  assert 'Finished `test` profile' in output and assertion in output and 'test result: FAILED. 0 passed; 1 failed;' in output,output
  assert 'left:' in output and 'right:' in output,output
  records[-1]['named_failure']=assertion
 elif label in ['baseline','final-restore']:assert 'test result: ok. 4 passed; 0 failed; 0 ignored;' in output,output
 elif label=='extractor-check':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output,output
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','position_refresh','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
source=crate/'src/position_refresh.rs'
for variant,suffix,prefix in [('binance','binance','futures'),('binancepm','binance_pm','pm')]:
 for method,function,before,after,pair in [('throttle',f'throttle_{suffix}_positions','state.throttle_ms.get() - elapsed','state.throttle_ms.get() - elapsed + 1.0','9901 versus 9900' if variant=='binance' else '401 versus 400'),('execute',f'execute_{suffix}_position_fetch','state.last_fetch_time.set(now()?);','state.last_fetch_time.set(now()? + 1.0);','1001 versus 1000')]:
  original=originals[source].decode();start=original.index(('pub fn ' if method=='throttle' else 'pub async fn ')+function+'<');end=original.find('\n// ts:',start)
  if end<0:end=len(original)
  body=original[start:end];assert body.count(before)==1
  label=f'{variant}-{method}-native-control';assertion=f'POSITION {variant} {method} native '+('delay' if method=='throttle' else 'last time')+' case 0'
  try:
   source.write_text(original[:start]+body.replace(before,after)+original[end:]);run(label,cargo+['--','--include-ignored',prefix+'_'+method,'--exact'],101,assertion)
   records[-1].update(call_site=str(source)+'::'+function,mutation_before=before,mutation_after=after,value_pair=pair)
  finally:source.write_bytes(originals[source])
  assert source.read_bytes()==originals[source]
 # Independently corrupt each actual source execute body, after all native cases.
 ts_path=Path('packages/safe-cex/src/exchanges/binance')/(variant+'.ws-private.ts');original=originals[ts_path].decode();start=original.index('  private executePositionFetch =');end=original.index('\n  };',start)+len('\n  };');body=original[start:end]
 before='this.lastPositionFetchTime = Date.now();';after='this.lastPositionFetchTime = Date.now() + 1;';assert body.count(before)==1
 try:
  ts_path.write_text(original[:start]+body.replace(before,after)+original[end:]);run(f'{variant}-execute-source-control',cargo+['--','--include-ignored',prefix+'_execute','--exact'],101,f'POSITION {variant} execute source case 0/last source decoded number')
  records[-1].update(call_site=str(ts_path)+'::executePositionFetch',mutation_before=before,mutation_after=after,value_pair='1001 versus 1000',native_assertions='All twenty-five native execute cases passed before the source comparison failed.')
 finally:ts_path.write_bytes(originals[ts_path])
 assert ts_path.read_bytes()==originals[ts_path]
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',str(source),str(crate/'tests/position_refresh.rs')])
run('git-diff-check',['git','diff','--check'])
for p,content in originals.items():assert p.read_bytes()==content,p
def census(revision):
 rows={}
 for p in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for row in tomllib.loads(subprocess.check_output(['git','show',f'{revision}:{p}'],text=True))['symbol']:
   key=(str(p),row['ts'],row['kind']);assert key not in rows;rows[key]=row['status']
 return rows
before,after=census(base),census(sha);assert before.keys()==after.keys()
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes)==4 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
evidence=dict(gate_sha=sha,parent_built_on=base,unreviewed_work_in_base=False,moved_pending_to_ported=4,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,records=records,restored_inputs_sha256={str(p):hashlib.sha256(v).hexdigest() for p,v in originals.items()},controls_surviving=[],enumeration_floor='94 independently authored cases: each family throttle22 stopping pause-mutates and execute25 stopping pending-after-reject. Six selected compiling controls, stopping PM execute TS last-time plus-one corruption. Mutated tests stop at first named case0 failure; no exhaustive input/mutation or whole-venue proof.',scope='Four completed-invocation bodies at configured fallible clock/pause/version/timer/fetch/store/current-method startup boundaries. Boolean/numeric state cells, identity-preserving native position handles and a fresh void timer wrapper. Starters resolve current methods and must start/retain jobs through settlement without awaiting discarded async rejection; only synchronous lookup/invocation errors are Result failures. Concrete class/arrow/timer/HTTP/Store composition, ongoing/discarded job ownership and cleanup after dropping or unwinding native futures remain pending.',environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),preliminary_uncommitted_runs=[dict(exit=101,passed=0,failed=4,ignored=0,reason='Two immediate-fail fixtures selected queued path because clock was100, plus two source/native diagnostic envelope key-order differences. Set both immediate-failure fixture clocks to independent threshold and aligned source envelope snapshot order; production bodies unchanged.'),dict(exit=0,passed=4,failed=0,ignored=0)],manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output='yarn run v1.22.21\n$ node -e "const p=\'sdk-rs/parity/extract-ts/src/cli.ts\';try{if(!require(\'node:fs\').statSync(p).isFile())throw Error()}catch{console.error(\'Parity extractor has not landed yet: \'+p);process.exit(1)}" && node --import tsx sdk-rs/parity/extract-ts/src/cli.ts\nSymbols: 7341 across 512 source files\nBy kind:\n  function: 313\n  method: 633\n  field-fn: 3006\n  constructor: 137\n  accessor: 115\n  class: 212\n  const-fn: 1214\n  const: 592\n  variable-fn: 0\n  variable: 3\n  type: 1081\n  enum: 35\nBy top-level directory:\n  (root): 146\n  accounts: 20\n  candleCache: 43\n  config: 11\n  crossTrade: 31\n  exchanges: 5866\n  health: 27\n  http-over-websocket: 102\n  httpOverWebsocket: 2\n  instrument-class: 20\n  journal: 33\n  json-parse: 2\n  lighter-sdk: 389\n  poseidon-crypto: 22\n  safe-cex-react-native: 3\n  safe-cex-server: 3\n  safe-cex-web: 11\n  script-api-core: 27\n  store: 83\n  transport: 63\n  utils: 215\n  webworker: 222\nUpdated 2 manifests; 0 added, 0 orphaned symbols.\nDone in 10.36s.\n'),not_run=['full crate test suite','workspace compilation','host clippy','wasm clippy','CI','concrete class/arrow/timer/HTTP/Store composition','ongoing/discarded async job ownership','cleanup after dropping or unwinding native futures','whole-venue audit','cross-engine runtime verification'])
(crate/'POSITION_REFRESH_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n');print('Evidence recorded at',sha,flush=True)
