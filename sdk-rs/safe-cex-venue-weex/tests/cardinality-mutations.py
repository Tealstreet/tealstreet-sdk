"""Serial production empty/subset controls; originals restored even on failure."""
from pathlib import Path
import os,re,subprocess
crate=Path(__file__).resolve().parent.parent
utils=crate/'src/utils.rs';types=crate/'src/types.rs';response=crate/'src/response_types.rs'
originals={p:p.read_text() for p in [utils,types,response]}
controls=[]
u=originals[utils]
for n in [0,1]:
 controls.append((utils,u.replace('    Ok(acc)\n','    acc.truncate('+str(n)+');\n    Ok(acc)\n'),'orderbook','book::normalizeOrderBookSide::distinct::route_0::length'))
 for side in ['bids','asks']:
  controls.append((utils,u.replace('    Ok(book)\n','    book.'+side+'.truncate('+str(n)+');\n    Ok(book)\n'),'orderbook','book::normalizeOrderBook::distinct::route_0::'+side+'::length'))
 start=u.index('    vec![\n        create_fake(PositionSide::Long),')
 end=u.index('\n}',start)
 controls.append((utils,u[:start]+u[start:end]+'.into_iter().take('+str(n)+').collect()'+u[end:],'fake_positions','fake_position::count'))
 before='&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]'
 controls.append((types,originals[types].replace(before,'&[]' if n==0 else '&[("BUY", OrderSide::Buy)]'),'types_oracle','constant::ORDER_SIDE::native'))
r=originals[response];start=r.index("impl<'de> Deserialize<'de> for WeexExchangeInfoResponse");end=r.index('impl JsNumberOutput for WeexExchangeInfoResponse',start)
for field in ['assets','symbols']:
 body=r[start:end].replace('            .map_err(',f'            .map(|mut value| {{ if let Some(rows) = &mut value.{field} {{ rows.clear(); }} value }})\n            .map_err(')
 controls.append((response,r[:start]+body+r[end:],'response_types','native::info_distinct::direct_raw'))
controls.append((response,r.replace('.decode_with(ExchangeInfoRecordWire::deserialize)','.decode_with(ExchangeInfoRecordWire::deserialize)\n            .map(|mut value| { if let Self::Array(rows) = &mut value { rows.truncate(1); } value })'),'response_types','native::info_array_row::direct_raw'))
for action in ['rows.clear();','if let Some(row) = rows.first().cloned() { rows.push(row); }']:
 controls.append((response,r.replace('.decode_with(ExchangeInfoRecordWire::deserialize)',f'.decode_with(ExchangeInfoRecordWire::deserialize)\n            .map(|mut value| {{ if let Self::Array(rows) = &mut value {{ {action} }} value }})'),'response_types','native::info_array_row::direct_raw'))
# Extra native outputs must also fail complete collection comparisons.
controls.append((utils,u.replace('    Ok(acc)\n','    if let Some(row) = acc.first().cloned() { acc.push(row); }\n    Ok(acc)\n'),'orderbook','book::normalizeOrderBookSide::distinct::route_0::length'))
for side in ['bids','asks']:
 controls.append((utils,u.replace('    Ok(book)\n',f'    if let Some(row) = book.{side}.first().cloned() {{ book.{side}.push(row); }}\n    Ok(book)\n'),'orderbook',f'book::normalizeOrderBook::distinct::route_0::{side}::length'))
controls.append((utils,u.replace('        create_fake(PositionSide::Short),','        create_fake(PositionSide::Short),\n        create_fake(PositionSide::Long),'),'fake_positions','fake_position::count'))
controls.append((types,originals[types].replace('&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]','&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell), ("BUY", OrderSide::Buy)]'),'types_oracle','constant::ORDER_SIDE::native'))
for field in ['assets','symbols']:
 body=r[start:end].replace('            .map_err(',f'            .map(|mut value| {{ if let Some(rows) = &mut value.{field} {{ rows.push(SelfRecord::Array(Vec::new())); }} value }})\n            .map_err(').replace('SelfRecord','WeexExchangeInfoRecord')
 controls.append((response,r[:start]+body+r[end:],'response_types','native::info_distinct::direct_raw'))
# Repeat every production mutation while all source census calls are disabled.
support=crate/'tests/support/cardinality.rs'; originals[support]=support.read_text()
for_bypass=originals[support].replace('pub fn check(corpus: &Value, name: &str) {','pub fn check(corpus: &Value, name: &str) { if std::hint::black_box(true) { return; }')
fake_test=crate/'tests/fake_positions.rs'; originals[fake_test]=fake_test.read_text()
fake_bypass=originals[fake_test]
for marker in ['source_total','source_per_case']:
 fake_bypass,count=re.subn(r'assert_eq!\([^;]*?"cardinality::positions::'+marker+r'"\s*\);','let _ = expected_total;' if marker=='source_total' else '',fake_bypass)
 assert count==1,marker
book_test=crate/'tests/orderbook.rs'; originals[book_test]=book_test.read_text()
try:
 for bypass,file,mutated,target,assertion in [(b,*c) for b in [False,True] for c in controls]:
  support.write_text(for_bypass if bypass else originals[support])
  fake_test.write_text(fake_bypass if bypass else originals[fake_test])
  assert mutated!=originals[file],assertion
  file.write_text(mutated)
  cmd=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target]
  run=subprocess.run(cmd,capture_output=True,text=True);output=run.stdout+run.stderr
  assert run.returncode==101 and 'assertion `left == right` failed' in output and assertion in output,output
  print('COMPILED exit101 '+('CENSUS_BYPASSED ' if bypass else '')+assertion,flush=True)
  file.write_text(originals[file])
 # Both sides empty with source census bypassed: the explicit work count must fail.
 support.write_text(for_bypass)
 fake_test.write_text(originals[fake_test])
 utils.write_text(u.replace('    Ok(acc)\n','    acc.clear();\n    Ok(acc)\n'))
 injected='    for case in corpus["cases"].as_array_mut().unwrap() { if let Some(result) = case.get_mut("result") { if let Some(rows) = result.as_array_mut() { rows.clear(); } else { result["bids"].as_array_mut().unwrap().clear(); result["asks"].as_array_mut().unwrap().clear(); } } }\n'
 book_test.write_text(originals[book_test].replace('let corpus: Value','let mut corpus: Value').replace('    cardinality::check(&corpus, "book");',injected+'    cardinality::check(&corpus, "book");'))
 run=subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','orderbook'],capture_output=True,text=True)
 output=run.stdout+run.stderr
 assert run.returncode==101 and 'cardinality::book::compared_levels' in output and 'assertion `left == right` failed' in output,output
 print('COMPILED exit101 BOTH_SIDES_EMPTY CENSUS_BYPASSED cardinality::book::compared_levels',flush=True)
finally:
 for file,original in originals.items():file.write_text(original)
for target in sorted({c[2] for c in controls}):
 subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target],check=True)
print(f'Restored PASS after {2*len(controls)+1} production cardinality controls',flush=True)
