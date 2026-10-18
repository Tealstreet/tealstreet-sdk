"""Consume every element in a production field decoder, then return empty."""
from pathlib import Path
import os,re,subprocess
from decoder_mutations import decoder_vectors
crate=Path(__file__).resolve().parents[1];source=crate/'src/types.rs';guard=crate/'tests/support/cardinality.rs'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','parity','--jobs','1','--','native_fields_and_bits','--exact']
original=source.read_bytes();text=original.decode();matches=decoder_vectors(text);assert len(matches)==13
helper='''
fn consume_every_element_then_empty<'de, D, T>(decoder: D) -> Result<Vec<T>, D::Error>
where D: serde::Deserializer<'de>, T: serde::Deserialize<'de> {
    let consumed = Vec::<T>::deserialize(decoder)?;
    drop(consumed);
    Ok(Vec::new())
}
'''
for index,match in enumerate(matches+[matches[0]]):
 bypass=index==len(matches);name=f'{match[1]}:Vec<{match[2]}>';guard_original=guard.read_bytes()
 try:
  mutated=text[:match.start()]+'    #[serde(deserialize_with = "consume_every_element_then_empty")]\n'+text[match.start():]+helper
  source.write_text(mutated)
  if bypass:
   g=guard_original.decode().replace('assert_eq!(source_census(value), expected,','assert_eq!({ let _ = source_census(value); expected.clone() }, expected,')
   marker='    match expected {';assert g.count(marker)==1
   g=g.replace(marker,'    if !path.is_empty() { return; }\n'+marker)
   guard.write_text(g)
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  named='DECODED-NATIVE full fields and cardinality' if bypass else 'NATIVE-CARDINALITY'
  assert r.returncode==101 and named in out and 'test result: FAILED' in out,(name,bypass,r.returncode,out)
  print(f'{name}; guards_bypassed={bypass}: compiled; exit101; {named}',flush=True)
 finally:source.write_bytes(original);guard.write_bytes(guard_original)
 r=subprocess.run(cmd,env=env,capture_output=True,text=True);assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}; guards_bypassed={bypass}: restored PASS',flush=True)
