"""Inject a consuming shared decoder mutant locally, restore all bytes afterward."""
from pathlib import Path
import os,re,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate.parent/'safe-cex-num/src/numeric_ingress.rs'
test=crate/'tests/response_types.rs'
original=source.read_text();baseline=test.read_text()
a=original.index("impl<'de> SeqAccess<'de> for Array")
b=original.index('    fn size_hint',a)
body=original[a:b]
old='''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
assert body.count(old)==1
mutated=original[:a]+body.replace(old,'''        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)''')+original[b:]
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','response_types','--','consuming_collection_decode_has_native_cardinality']
no_lengths=baseline
for field in ['assets','symbols']:
 no_lengths,n=re.subn(r'assert_eq!\(\s*'+field+r'\.len\(\),\s*1,\s*"consuming::'+field+r'::native_length"\s*\);','',no_lengths)
 assert n==1,field
no_guards,n=re.subn(r'assert_eq!\(\s*compared,\s*2,\s*"consuming::records::compared"\s*\);','let _ = compared;',no_lengths)
assert n==1
try:
 source.write_text(mutated)
 for contents,assertion in [(baseline,'consuming::assets::native_length'),(no_lengths,'consuming::records::compared')]:
  test.write_text(contents);r=subprocess.run(command,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'assertion `left == right` failed' in out,out
  print('COMPILED consuming-empty exit101 '+assertion,flush=True)
 test.write_text(baseline)
 for target in ['orderbook','candles','numeric_utils','utils_oracle','timestamps']:
  scoped=command[:command.index('--test')]+['--test',target]
  r=subprocess.run(scoped,capture_output=True,text=True);out=r.stdout+r.stderr
  named=re.findall(r'assertion `left == right` failed: ([^\n]+)',out)
  if target=='utils_oracle':
   named=['utility::normalizePositionMode::\"json_26\"::route_1'] if 'utility::normalizePositionMode::\"json_26\"::route_1: OneWay' in out else []
  assert r.returncode==101 and named,out
  print('COMPILED consuming-empty '+target+' exit101 '+named[0],flush=True)
 test.write_text(no_guards);r=subprocess.run(command,capture_output=True,text=True)
 assert r.returncode==0,r.stdout+r.stderr
 print('GUARD-REMOVAL CONTROL: consuming-empty PASSED only after both lengths AND comparison count removed',flush=True)
finally:
 source.write_text(original);test.write_text(baseline)
subprocess.run(command,check=True)
for target in ['orderbook','candles','numeric_utils','utils_oracle','timestamps']:
 subprocess.run(command[:command.index('--test')]+['--test',target],check=True)
print('Shared decoder and all guards restored; native cardinality probe PASS',flush=True)
