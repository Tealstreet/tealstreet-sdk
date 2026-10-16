#!/usr/bin/env python3
"""Serial SHA-bound eight-row verification: eight native controls, two source controls."""
import argparse,hashlib,json,os,platform,subprocess,tomllib
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--gate-sha',required=True);parser.add_argument('--base-sha',required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[3];os.chdir(root);crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse',args.gate_sha],text=True).strip();base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha;subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-public-tickers')/sha;logs.mkdir(parents=True,exist_ok=True)
inputs=[crate/'src/public_ticker_events.rs',crate/'src/spot_ticker_events.rs',crate/'src/lib.rs',crate/'tests/public_ticker_events.rs',crate/'scripts/public-ticker-events.mjs',Path(__file__).relative_to(root),Path('packages/safe-cex/src/utils/get-key.ts'),Path('packages/safe-cex/src/exchanges/base.ws.ts')]
for venue in ['binance','binancepm','binancespot']:inputs += [Path('sdk-rs/parity/manifest/exchanges/binance')/(venue+'.ws-public.ts.toml'),Path('packages/safe-cex/src/exchanges/binance')/(venue+'.ws-public.ts')]
originals={p:p.read_bytes() for p in inputs}
for p,content in originals.items():assert content==subprocess.check_output(['git','show',f'{sha}:{p}']),p
records=[]
def run(label,cmd,expected=0,assertion=None):
 p=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_bytes(p.stdout);output=p.stdout.decode(errors='replace')
 records.append(dict(label=label,gate_sha=sha,command=cmd,exit=p.returncode,output=output,log_sha256=hashlib.sha256(p.stdout).hexdigest()));print(label,'exit',p.returncode,flush=True);assert p.returncode==expected,output
 if assertion:
  assert 'Finished `test` profile' in output and assertion in output and 'test result: FAILED. 0 passed; 1 failed;' in output,output
  assert 'left:' in output and 'right:' in output,output;records[-1]['named_failure']=assertion
 elif label in ['baseline','final-restore']:assert 'test result: ok. 8 passed; 0 failed; 0 ignored;' in output,output
 elif label=='extractor-check':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output,output
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','public_ticker_events','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
lanes=['futures_ticker','futures_mark','futures_book','pm_ticker','pm_mark','pm_book','spot_ticker','spot_book']
for lane in lanes:
 source=crate/'src'/('spot_ticker_events.rs' if lane.startswith('spot') else 'public_ticker_events.rs');original=originals[source].decode();function='handle_'+lane+'_events';start=original.index('pub fn '+function+'<');end=original.find('\n// ts:',start)
 if end<0:end=len(original)
 body=original[start:end];key='b' if lane.endswith('book') else 'p' if lane.endswith('mark') else 'c';before=f'parse_field(&event, "{key}")?';after=before+' + 1.0';assert body.count(before)>=1
 try:
  source.write_text(original[:start]+body.replace(before,after,1)+original[end:]);run(lane+'-native-first-field-plus-one',cargo+['--','--include-ignored',lane,'--exact'],101,f'TICKER {lane} native value case0 row0 field0')
  records[-1].update(call_site=str(source)+'::'+function,mutation_before=before,mutation_after=after,replace_occurrence='first only, preserving Spot repeated c reads',value_pair='13.2 versus12.2' if key=='b' else '14.1 versus13.1' if key=='p' else '13.3 versus12.3')
 finally:source.write_bytes(originals[source])
 assert source.read_bytes()==originals[source]
# Two actual TS body corruptions independently prove source extraction freshness.
for lane,venue,klass,member,field,key in [('futures_ticker','binance','BinanceMarketWebsocket','handleTickerStreamEvents','last','c'),('spot_book','binancespot','BinancePublicWebsocket','handleBookTickersStreamEvents','bid','b')]:
 source=Path('packages/safe-cex/src/exchanges/binance')/(venue+'.ws-public.ts');original=originals[source].decode();class_start=original.index('export class '+klass+' ');start=original.index('  '+member+' =',class_start);end=original.index('\n  };',start)+len('\n  };');body=original[start:end];before=f"{field}: parseFloat(v(event, '{key}'))";after=before+' + 1';assert body.count(before)==1
 try:
  source.write_text(original[:start]+body.replace(before,after)+original[end:]);run(lane+'-source-first-field-plus-one',cargo+['--','--include-ignored',lane,'--exact'],101,f'TICKER {lane} source case0/updates/0/fields/0/1 source decoded number')
  records[-1].update(call_site=str(source)+'::'+klass+'.'+member,mutation_before=before,mutation_after=after,value_pair='13.3 versus12.3' if key=='c' else '13.2 versus12.2',native_assertions='All twenty-eight authored native cases passed before the source comparison failed.')
 finally:source.write_bytes(originals[source])
 assert source.read_bytes()==originals[source]
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',str(crate/'src/public_ticker_events.rs'),str(crate/'src/spot_ticker_events.rs'),str(crate/'tests/public_ticker_events.rs')])
run('git-diff-check',['git','diff','--check'])
for p,content in originals.items():assert p.read_bytes()==content,p
def census(revision):
 rows={}
 for p in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
  for row in tomllib.loads(subprocess.check_output(['git','show',f'{revision}:{p}'],text=True))['symbol']:
   key=(str(p),row['ts'],row['kind']);assert key not in rows;rows[key]=row['status']
 return rows
before,after=census(base),census(sha);assert before.keys()==after.keys();changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes)==8 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
evidence=dict(gate_sha=sha,parent_built_on=base,unreviewed_work_in_base=False,moved_pending_to_ported=8,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,records=records,restored_inputs_sha256={str(p):hashlib.sha256(v).hexdigest() for p,v in originals.items()},controls_surviving=[],enumeration_floor='224 independently authored cases: eight handlers each28, stopping false-ticker. Ten selected compiling controls: eight native first-field plus-one, then Futures ticker and Spot book actual TS first-field plus-one; stopped iteration10 at Spot book sourcecase0. Mutated tests stop at named case0; no exhaustive input/mutation or whole-venue proof.',scope='Eight synchronous bodies: once-captured live dictionary handle; live decoded event arrays/entries, undefined events and JSON data fields with stock prototypes; property-key String and real shared v/parseFloat; lookup callback normalizes all JavaScript-falsy entries toNone; current Store.updateTicker method+receiver capture before ordered nativef64 parsing; original ticker handles and partial effects. Concrete class/Store/dictionary wiring, inherited dictionary lookups/access beyond configured callbacks, own undefined-valued fields, executable property hooks and non-JSON mutation/coercion remain pending.',environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),preliminary_uncommitted_runs=[dict(exit=0,passed=8,failed=0,ignored=0)],manifest_writers=[{'command': ['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract'], 'exit': 0, 'output': 'yarn run v1.22.21\n$ node -e "const p=\'sdk-rs/parity/extract-ts/src/cli.ts\';try{if(!require(\'node:fs\').statSync(p).isFile())throw Error()}catch{console.error(\'Parity extractor has not landed yet: \'+p);process.exit(1)}" && node --import tsx sdk-rs/parity/extract-ts/src/cli.ts\nSymbols: 7341 across 512 source files\nBy kind:\n  function: 313\n  method: 633\n  field-fn: 3006\n  constructor: 137\n  accessor: 115\n  class: 212\n  const-fn: 1214\n  const: 592\n  variable-fn: 0\n  variable: 3\n  type: 1081\n  enum: 35\nBy top-level directory:\n  (root): 146\n  accounts: 20\n  candleCache: 43\n  config: 11\n  crossTrade: 31\n  exchanges: 5866\n  health: 27\n  http-over-websocket: 102\n  httpOverWebsocket: 2\n  instrument-class: 20\n  journal: 33\n  json-parse: 2\n  lighter-sdk: 389\n  poseidon-crypto: 22\n  safe-cex-react-native: 3\n  safe-cex-server: 3\n  safe-cex-web: 11\n  script-api-core: 27\n  store: 83\n  transport: 63\n  utils: 215\n  webworker: 222\nUpdated 3 manifests; 0 added, 0 orphaned symbols.\nDone in 11.07s.\n'}, {'command': ['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract'], 'exit': 0, 'output': 'yarn run v1.22.21\n$ node -e "const p=\'sdk-rs/parity/extract-ts/src/cli.ts\';try{if(!require(\'node:fs\').statSync(p).isFile())throw Error()}catch{console.error(\'Parity extractor has not landed yet: \'+p);process.exit(1)}" && node --import tsx sdk-rs/parity/extract-ts/src/cli.ts\nSymbols: 7341 across 512 source files\nBy kind:\n  function: 313\n  method: 633\n  field-fn: 3006\n  constructor: 137\n  accessor: 115\n  class: 212\n  const-fn: 1214\n  const: 592\n  variable-fn: 0\n  variable: 3\n  type: 1081\n  enum: 35\nBy top-level directory:\n  (root): 146\n  accounts: 20\n  candleCache: 43\n  config: 11\n  crossTrade: 31\n  exchanges: 5866\n  health: 27\n  http-over-websocket: 102\n  httpOverWebsocket: 2\n  instrument-class: 20\n  journal: 33\n  json-parse: 2\n  lighter-sdk: 389\n  poseidon-crypto: 22\n  safe-cex-react-native: 3\n  safe-cex-server: 3\n  safe-cex-web: 11\n  script-api-core: 27\n  store: 83\n  transport: 63\n  utils: 215\n  webworker: 222\nUpdated 3 manifests; 0 added, 0 orphaned symbols.\nDone in 10.11s.\n'}],not_run=['full crate test suite','workspace compilation','host clippy','wasm clippy','CI','concrete class/Store/dictionary wiring','inherited dictionary lookups beyond configured callback','own undefined-valued fields/non-JSON mutation/coercion','executable property hooks','whole-venue audit','cross-engine runtime verification'])
(crate/'PUBLIC_TICKER_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n');print('Evidence recorded at',sha,flush=True)
