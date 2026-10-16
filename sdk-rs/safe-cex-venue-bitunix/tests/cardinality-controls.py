from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[3]; crate=root/'sdk-rs/safe-cex-venue-bitunix'; logs=root/'sdk-rs/target/bitunix-cardinality'; logs.mkdir(exist_ok=True)
wire=crate/'src/wire.rs'; oracle=crate/'tests/wire-oracle.cjs'; constants=crate/'src/types.rs'
test=crate/'tests/wire.rs'
supplement=crate/'tests/wire-cardinality-oracle.cjs'
original={p:p.read_text() for p in [wire,oracle,constants,test,supplement]}
alias='pub type GetPendingPositionsResponse = Vec<PendingPosition>;'
def replacement(operation):
 return '''#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct GetPendingPositionsResponse(Vec<PendingPosition>);
impl std::ops::Deref for GetPendingPositionsResponse {
 type Target = Vec<PendingPosition>;
 fn deref(&self)->&Self::Target { &self.0 }
}
impl<'de> Deserialize<'de> for GetPendingPositionsResponse {
 fn deserialize<D:serde::Deserializer<'de>>(decoder:D)->Result<Self,D::Error> {
  let mut rows=Vec::<PendingPosition>::deserialize(decoder)?;
  '''+operation+'''; Ok(Self(rows))
 }
}
impl JsNumberOutput for GetPendingPositionsResponse {
 fn check_number_output(&self) { self.0.check_number_output(); }
}'''

import re
source_missing=original[supplement].replace("native:moduleRecord.exports.capture(value).map", "native:moduleRecord.exports.capture(value).slice(0,1).map")
without_source=re.sub(r'    // SOURCE_OUTPUT_CARDINALITY_START.*?    // SOURCE_OUTPUT_CARDINALITY_END', '', original[test], flags=re.S)
controls=[
 ('native-empty',{wire:original[wire].replace(alias,replacement('rows.clear()'))},'wire','wire native response count'),
 ('native-subset',{wire:original[wire].replace(alias,replacement('rows.truncate(1)'))},'wire','wire native response count'),
 ('oracle-empty',{oracle:original[oracle].replace('{ fields, cases, intervals: interval.types.map(t => t.value) }','{ fields, cases: [], intervals: [] }')},'wire','wire source case count'),
 ('oracle-subset',{oracle:original[oracle].replace('{ fields, cases, intervals: interval.types.map(t => t.value) }','{ fields, cases: cases.slice(1), intervals: interval.types.map(t => t.value) }')},'wire','wire source case count'),
 ('source-output-subset',{supplement:source_missing},'wire','wire source per-case output count'),
 ('source-count-bypassed',{supplement:source_missing,test:without_source},'wire','wire native response count'),
 ('constant-empty',{constants:original[constants].replace('&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]','&[]')},'constants','constant witness ORDER_SIDE'),
 ('constant-subset',{constants:original[constants].replace('&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]','&[("BUY", OrderSide::Buy)]')},'constants','constant witness ORDER_SIDE'),
]
for name,changes,target,assertion in controls:
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--jobs','1','--test',target,'--','--nocapture']
 try:
  for p,changed in changes.items():
   assert changed!=original[p],name
   p.write_text(changed)
  r=subprocess.run(command,text=True,capture_output=True)
  (logs/(name+'-fail.log')).write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'Finished `test`' in r.stderr and assertion in r.stdout+r.stderr,(name,r.returncode,r.stdout+r.stderr)
  print('FAIL compiled 101: '+assertion,flush=True)
 finally:
  for p in changes:p.write_text(original[p])
 r=subprocess.run(command,text=True,capture_output=True)
 (logs/(name+'-pass.log')).write_text(r.stdout+r.stderr)
 assert r.returncode==0,(name,r.stdout+r.stderr)
 print('PASS restored: '+name,flush=True)
