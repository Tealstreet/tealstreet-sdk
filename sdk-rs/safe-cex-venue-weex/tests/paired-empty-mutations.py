"""Both collection sides empty, source census bypassed, native inventory retained."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
support=crate/'tests/support/cardinality.rs';types=crate/'src/types.rs';response=crate/'src/response_types.rs'
type_test=crate/'tests/types_oracle.rs';response_test=crate/'tests/response_types.rs'
originals={p:p.read_text() for p in [support,types,response,type_test,response_test]}
checks=[]
t=originals[type_test].replace('let expected: Value','let mut expected: Value').replace('    cardinality::check(&expected, "types");','    expected["constants"]["ORDER_SIDE"] = json!([]);\n    cardinality::check(&expected, "types");')
checks.append(({types:originals[types].replace('&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]','&[]'),type_test:t},'types_oracle','constants_match_actual_typescript','cardinality::types::ORDER_SIDE::native_entries'))
t=originals[type_test].replace('let expected: Value','let mut expected: Value').replace('    cardinality::check(&expected, "types");','    for entry in expected["constants"]["BASE_WS_URL"].as_array_mut().unwrap() { entry[1] = json!([]); }\n    cardinality::check(&expected, "types");')
native=originals[types]
start=native.index('pub const BASE_WS_URL:');end=native.index('pub const PRIVATE_WS_REQUEST_PATH:',start)
native=native[:start]+'pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[("public", &[]), ("private", &[])];\n\n'+native[end:]
checks.append(({types:native,type_test:t},'types_oracle','constants_match_actual_typescript','cardinality::types::BASE_WS_URL::native_shape'))
r=originals[response];start=r.index("impl<'de> Deserialize<'de> for WeexExchangeInfoResponse");end=r.index('impl JsNumberOutput for WeexExchangeInfoResponse',start)
body=r[start:end].replace('            .map_err(','            .map(|mut value| { if let Some(rows) = &mut value.assets { rows.clear(); } value })\n            .map_err(')
t=originals[response_test];cut=t.index('fn response_types_match_actual_typescript()');prefix,t=t[:cut],t[cut:]
inject='    for case in expected["rows"].as_array_mut().unwrap() { if case["type"] == "WeexExchangeInfoResponse" { if let Some(entries) = case["capture"]["entries"].as_array_mut() { for entry in entries { if entry[0] == "assets" { entry[1]["values"] = json!([]); } } } } }\n'
t=prefix+t.replace('let expected: Value','let mut expected: Value').replace('    cardinality::check(&expected, "response");',inject+'    cardinality::check(&expected, "response");')
checks.append(({response:r[:start]+body+r[end:],response_test:t},'response_types','response_types_match_actual_typescript','cardinality::response::info_distinct::direct_raw::native_shape'))
try:
 for edits,target,test,assertion in checks:
  support.write_text(originals[support].replace('pub fn check(corpus: &Value, name: &str) {','pub fn check(corpus: &Value, name: &str) { if std::hint::black_box(true) { return; }'))
  for path,content in edits.items():path.write_text(content)
  r=subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target,'--',test],capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'assertion `left == right` failed' in out,out
  print('COMPILED BOTH_SIDES_EMPTY CENSUS_BYPASSED exit101 '+assertion,flush=True)
  for path,content in originals.items():path.write_text(content)
finally:
 for path,content in originals.items():path.write_text(content)
for target in ['types_oracle','response_types']:
 subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target],check=True)
print('Restored PASS after 3 paired-empty source-bypass controls',flush=True)
