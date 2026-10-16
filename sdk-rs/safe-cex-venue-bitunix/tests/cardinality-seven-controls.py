"""Guard-bypass group controls and exact consuming SeqAccess mutations."""
from pathlib import Path
import re,subprocess
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-venue-bitunix'
logs=root/'sdk-rs/target/bitunix-cardinality-seven';logs.mkdir(exist_ok=True)
constants=crate/'tests/constants.rs';test=crate/'tests/wire.rs';wire=crate/'src/wire.rs'
original={p:p.read_text() for p in [constants,test,wire]}
controls=[]
for mode,operation in [('empty','oracle.as_object_mut().unwrap().clear();'),('subset','oracle.as_object_mut().unwrap().remove("RECV_WINDOW");'),('extra','oracle.as_object_mut().unwrap().insert("EXTRA".into(), Value::Null);')]:
 changed=original[constants].replace('let oracle: Value','let mut oracle: Value').replace('    let native = json!({','    '+operation+'\n    let native = json!({')
 changed,n=re.subn(r'    assert_eq!\(\n        native.*?"constant export census"\n    \);','',changed,flags=re.S);assert n==1
 controls.append(('constant-group-'+mode,{constants:changed},'constants','constant actual export comparison count'))
for mode,operation in [('empty','corpus["intervals"] = json!([]);'),('subset','corpus["intervals"].as_array_mut().unwrap().pop();'),('extra','let extra = corpus["intervals"][0].clone(); corpus["intervals"].as_array_mut().unwrap().push(extra);')]:
 changed=original[test].replace('let corpus: Value','let mut corpus: Value').replace('    // SOURCE_CARDINALITY_START','    '+operation+'\n    // SOURCE_CARDINALITY_START')
 changed,n=re.subn(r'    // SOURCE_CARDINALITY_START.*?    // SOURCE_CARDINALITY_END','',changed,flags=re.S);assert n==1
 changed,n=re.subn(r'    // SOURCE_OUTPUT_CARDINALITY_START.*?    // SOURCE_OUTPUT_CARDINALITY_END','',changed,flags=re.S);assert n==1
 controls.append(('interval-group-'+mode,{test:changed},'wire','wire native interval decode count'))
mutant='''#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct GetPendingPositionsResponse(Vec<PendingPosition>);
impl std::ops::Deref for GetPendingPositionsResponse {
 type Target = Vec<PendingPosition>;
 fn deref(&self)->&Self::Target { &self.0 }
}
struct DrainBeforeNone<A>(A);
impl<'de, A:serde::de::SeqAccess<'de>> serde::de::SeqAccess<'de> for DrainBeforeNone<A> {
 type Error=A::Error;
 fn next_element_seed<T:serde::de::DeserializeSeed<'de>>(&mut self,_seed:T)->Result<Option<T::Value>,Self::Error> {
  while self.0.next_element::<serde::de::IgnoredAny>()?.is_some() {}
  Ok(None)
 }
}
struct ResponseVisitor;
impl<'de> serde::de::Visitor<'de> for ResponseVisitor {
 type Value=GetPendingPositionsResponse;
 fn expecting(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("position array")}
 fn visit_seq<A:serde::de::SeqAccess<'de>>(self,seq:A)->Result<Self::Value,A::Error> {
  Vec::<PendingPosition>::deserialize(serde::de::value::SeqAccessDeserializer::new(DrainBeforeNone(seq))).map(GetPendingPositionsResponse)
 }
}
impl<'de> Deserialize<'de> for GetPendingPositionsResponse {
 fn deserialize<D:serde::Deserializer<'de>>(decoder:D)->Result<Self,D::Error> {
  decoder.deserialize_seq(ResponseVisitor)
 }
}
impl JsNumberOutput for GetPendingPositionsResponse {
 fn check_number_output(&self) {self.0.check_number_output();}
}'''
changed=original[wire].replace('pub type GetPendingPositionsResponse = Vec<PendingPosition>;',mutant)
assert changed!=original[wire]
controls.append(('consuming-next-element-none',{wire:changed},'wire','wire native response count'))
bypass,n=re.subn(r'    // SOURCE_OUTPUT_CARDINALITY_START.*?    // SOURCE_OUTPUT_CARDINALITY_END','',original[test],flags=re.S);assert n==1
bypass,n=re.subn(r'                assert_eq!\(\n                    rows.len\(\).*?                \);','',bypass,flags=re.S);assert n==1
controls.append(('consuming-none-both-lengths-bypassed',{wire:changed,test:bypass},'wire','position decoded fields'))
for name,changes,target,assertion in controls:
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--jobs','1','--test',target,'--','--nocapture']
 try:
  for p,changed in changes.items():
   assert changed!=original[p],name
   p.write_text(changed)
  r=subprocess.run(command,text=True,capture_output=True)
  (logs/(name+'-fail.log')).write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'Finished `test`' in r.stderr and assertion in r.stdout+r.stderr,(name,r.returncode,r.stdout+r.stderr)
  print('FAIL compiled 101: '+name+' / '+assertion,flush=True)
 finally:
  for p in changes:p.write_text(original[p])
 r=subprocess.run(command,text=True,capture_output=True)
 (logs/(name+'-pass.log')).write_text(r.stdout+r.stderr)
 assert r.returncode==0,(name,r.stdout+r.stderr)
 print('PASS restored: '+name,flush=True)
print('controls proved:',len(controls),flush=True)
