#!/usr/bin/env python3
"""Serial empty/subset production controls; one Node corpus invocation per target run."""
from pathlib import Path
import os, subprocess, hashlib
crate = Path(__file__).resolve().parents[1]
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--locked', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--jobs', '1']
sites = [
 ('fill-batch', 'utilities', 'src/fills.rs', '    Ok(fills)', 'fills', 'fills.mapKrakenFills decoded native fields', 0),
 ('income-row', 'utilities', 'src/income.rs', '    Ok(records)', 'records', 'income.mapKrakenAccountLogRow decoded native fields', 0),
 ('income-batch', 'utilities', 'src/income.rs', '    Ok(records)', 'records', 'income.mapKrakenAccountLog decoded native fields', 1),
 ('spot-positions', 'reads', 'src/reads.rs', '    positions\n}', 'positions', 'Spot positions native fields and order', 0),
]
controls=[]
for name,target,file,anchor,collection,assertion,index in sites:
 for mode,operation in [('empty','clear()'),('subset','pop()')]:
  controls.append((name+'-'+mode,target,file,anchor,f'    {collection}.{operation};\n'+anchor,assertion,index))
anchor='        income::map_kraken_account_log(\n            js_from_value::<JsNumberInput>(logs)?,\n            &mut resolve,\n            local_to_utc,\n        )'
for mode,operation in [('empty','clear()'),('subset','pop()')]:
 controls.append(('account-log-'+mode,'reads','src/reads.rs',anchor,anchor+f'.map(|mut rows| {{ rows.{operation}; rows }})','account-log native records and error',0))
anchor='pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];'
for mode,items in [('empty',''),('subset','("BUY", OrderSide::Buy)')]:
 controls.append(('table-'+mode,'types','src/types.rs',anchor,f'pub const ORDER_SIDE: &[(&str, OrderSide)] = &[{items}];','table futures.ORDER_SIDE',0))
anchor='pub const PUBLIC_ENDPOINTS: &[&str] = &["/v3/instruments", "/v3/tickers", "/v3/orderbook"];'
for mode,items in [('empty',''),('subset','"/v3/instruments"')]:
 controls.append(('array-'+mode,'types','src/types.rs',anchor,f'pub const PUBLIC_ENDPOINTS: &[&str] = &[{items}];','array futures.PUBLIC_ENDPOINTS',0))
# The original 14 controls proved complete-array equality rejected short outputs.
original_count = len(controls)
anchor = '        Ok(token.expect("truthy token is present").clone())'
for mode,operation in [('empty','clear()'),('subset','pop()')]:
 controls.append(('array-token-'+mode,'reads','src/reads.rs',anchor,'        let mut token = token.expect("truthy token is present").clone();\n        if let Value::Array(values) = &mut token { values.'+operation+'; }\n        Ok(token)','KRAKEN-CARDINALITY independent native length token native value/error',0))
# Apply source-guard bypass to every concrete production empty/subset control.
for name,target,file,old,new,assertion,index in list(controls):
 controls.append((name+'-source-guards-bypassed',target,file,old,new,assertion,index))
for name,target,file,anchor,collection,assertion,index in sites:
 controls.append((name+'-extra',target,file,anchor,f'    if let Some(extra) = {collection}.first().cloned() {{ {collection}.push(extra); }}\n'+anchor,assertion,index))
anchor='        income::map_kraken_account_log(\n            js_from_value::<JsNumberInput>(logs)?,\n            &mut resolve,\n            local_to_utc,\n        )'
controls.append(('account-log-extra','reads','src/reads.rs',anchor,anchor+'.map(|mut rows| { if let Some(extra) = rows.first().cloned() { rows.push(extra); } rows })','account-log native records and error',0))
anchor='pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];'
controls.append(('table-extra','types','src/types.rs',anchor,anchor.replace('("SELL", OrderSide::Sell)', '("SELL", OrderSide::Sell), ("EXTRA", OrderSide::Buy)'),'table futures.ORDER_SIDE',0))
anchor='pub const PUBLIC_ENDPOINTS: &[&str] = &["/v3/instruments", "/v3/tickers", "/v3/orderbook"];'
controls.append(('array-extra','types','src/types.rs',anchor,anchor.replace('"/v3/orderbook"','"/v3/orderbook", "/extra"'),'array futures.PUBLIC_ENDPOINTS',0))
anchor='        Ok(token.expect("truthy token is present").clone())'
controls.append(('array-token-extra','reads','src/reads.rs',anchor,'        let mut token = token.expect("truthy token is present").clone();\n        if let Value::Array(values) = &mut token { values.push(Value::Null); }\n        Ok(token)','KRAKEN-CARDINALITY independent native length token native value/error',0))
for name,path in [('export','["futures"]["ORDER_SIDE"]'),('nested-ws','["futures"]["BASE_WS_URL"][0][1]')]:
 for mode,operation in [('empty','clear()'),('subset','pop()')]:
  anchor='        cardinality::source_census("types", &value);'
  controls.append((f'source-{name}-{mode}','types','tests/types.rs',anchor,'        let mut value = value;\n        value'+path+'.as_array_mut().unwrap().'+operation+';\n'+anchor,'KRAKEN-CARDINALITY source total and per-case counts',0))
for target,corpus in [('types','normalize'),('utilities','fills'),('helpers','triggers'),('reads','positions')]:
 anchor=f'        cardinality::source_census("{target}", &value);'
 for mode,operation in [('empty','clear()'),('subset','pop()')]:
  replacement=f'        let mut value = value;\n        value["{corpus}"].as_array_mut().unwrap().{operation};\n'+anchor
  controls.append((f'source-{target}-{mode}',target,f'tests/{target}.rs',anchor,replacement,'KRAKEN-CARDINALITY source total and per-case counts',0))
for bypass in [False,True]:
 for mode,operation in [('empty','clear()'),('subset','truncate(1)')]:
  anchor='        cardinality::source_census("helpers", &value);'
  replacement='        let mut value = value;\n        value["ids"][0]["cases"].as_array_mut().unwrap().'+operation+';\n'+('        let _ = cardinality::source_census as fn(&str, &Value);' if bypass else anchor)
  assertion='KRAKEN-CARDINALITY ID expected/native index coverage' if bypass else 'KRAKEN-CARDINALITY source total and per-case counts'
  controls.append((f'nested-ID-{mode}-bypass-{bypass}','helpers','tests/helpers.rs',anchor,replacement,assertion,0))
# Bypass the source census while dropping native rows: native cardinality remains independent.
for mode,operation in [('empty','clear()'),('subset','pop()')]:
 anchor='        let positions = reads::fetch_spot_positions(&store);'
 controls.append((f'native-independent-{mode}','reads','tests/reads.rs',anchor,anchor.replace('let positions','let mut positions')+f'\n        positions.{operation};','KRAKEN-CARDINALITY independent native length Spot positions native fields and order',0))
# Direct witness controls for the sole zip: these mutate its native/expected
# enumerations, not production enum definitions (removing a used variant cannot compile).
anchor='    let variants = [KrakenBaseUrl::Derivatives, KrakenBaseUrl::History];'
for mode,new in [('empty','    let variants: [KrakenBaseUrl; 0] = [];'),('subset','    let variants = [KrakenBaseUrl::Derivatives];'),('extra','    let variants = [KrakenBaseUrl::Derivatives, KrakenBaseUrl::History, KrakenBaseUrl::History];')]:
 controls.append(('zip-native-'+mode,'types','tests/types.rs',anchor,new,'KrakenBaseUrl literal census',0))
for mode,operation in [('empty','clear()'),('subset','pop()')]:
 anchor='        cardinality::source_census("types", &value);'
 new='        let mut value = value;\n        value["variants"].as_array_mut().unwrap().'+operation+';\n        let _ = cardinality::source_census as fn(&str, &Value);'
 controls.append(('zip-source-bypassed-'+mode,'types','tests/types.rs',anchor,new,'KrakenBaseUrl literal census',0))
import argparse
parser=argparse.ArgumentParser()
parser.add_argument('--new-only',action='store_true')
parser.add_argument('--six-shapes',action='store_true')
parser.add_argument('--zip-only',action='store_true')
parser.add_argument('--start-at', choices=[c[0] for c in controls])
args=parser.parse_args()
if args.new_only: controls=controls[original_count:]
if args.zip_only: controls=[c for c in controls if c[0].startswith('zip-')]
if args.six_shapes: controls=[c for c in controls if not c[0].startswith('zip-') and (c[0].endswith(('-source-guards-bypassed','-extra')) or c[0].startswith(('source-export-','source-nested-ws-')))]
if args.start_at: controls=controls[next(i for i,c in enumerate(controls) if c[0]==args.start_at):]
def run(target):
 return subprocess.run(command+['--test',target],cwd=crate.parent,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
for target in dict.fromkeys(c[1] for c in controls):
 baseline=run(target)
 assert baseline.returncode==0,baseline.stdout
for name,target,file,old,new,assertion,index in controls:
 path=crate/file
 original=path.read_bytes()
 source=original.decode()
 occurrences=source.count(old)
 assert occurrences == (2 if file=='src/income.rs' else 1),(name,occurrences)
 offset=-1
 for _ in range(index+1): offset=source.index(old,offset+1)
 secondary = crate/f'tests/{target}.rs'
 secondary_original = secondary.read_bytes() if name.endswith('-source-guards-bypassed') else None
 try:
  if secondary_original is not None:
   guard=f'        cardinality::source_census("{target}", &value);'
   text=secondary_original.decode()
   assert text.count(guard)==1
   secondary.write_text(text.replace(guard,'        let _ = cardinality::source_census as fn(&str, &Value);'))
  mutated=source[:offset]+new+source[offset+len(old):]
  if name.startswith('native-independent-'):
   mutated=mutated.replace('        cardinality::source_census("reads", &value);','        let _ = cardinality::source_census as fn(&str, &Value);')
  path.write_text(mutated)
  result=run(target)
  assert result.returncode==101 and 'test result: FAILED' in result.stdout and assertion in result.stdout,(name,result.returncode,result.stdout)
  print(f'{name}: compiled; exit 101; {assertion}',flush=True)
 finally:
  path.write_bytes(original)
  if secondary_original is not None: secondary.write_bytes(secondary_original)
 assert path.read_bytes()==original
 restored=run(target)
 assert restored.returncode==0,(name,restored.stdout)
 print(f'{name}: restored pass; sha256 {hashlib.sha256(original).hexdigest()}',flush=True)
print(f'{len(controls)} compiling named failures; {len(controls)} restored passes',flush=True)
