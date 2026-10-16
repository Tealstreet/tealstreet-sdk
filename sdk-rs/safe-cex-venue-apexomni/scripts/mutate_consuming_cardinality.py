"""Keep a consuming-empty decoder mutant installed while removing length guards.
The surviving whole-value failure proves redundant detection, not length-only coverage.
"""
from pathlib import Path
import os
import subprocess
ROOT=Path(__file__).resolve().parents[3]
P=ROOT/'sdk-rs/safe-cex-venue-apexomni'
source=P/'src/private_records.rs';test=P/'tests/records.rs'
original_source,original_test=source.read_text(),test.read_text()
cmd=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--locked','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--jobs','1','--test','records','collection_cardinality_native_supplement','--','--nocapture']
def run(label,code,assertion):
 r=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True);out=r.stdout+r.stderr
 (ROOT/'sdk-rs/target/apexomni-controls'/f'{label}.log').write_text(out)
 assert r.returncode==code and assertion in out,(label,out)
 if code==101:assert 'test result: FAILED.' in out
 print(label,r.returncode,assertion,flush=True)
try:
 needle='wallets: fields.wallets,';assert original_source.count(needle)==1
 # Fields::deserialize consumed every input value before this production assignment.
 source.write_text(original_source.replace(needle,'wallets: fields.wallets.into_iter().take(0).collect(),'))
 run('consuming-empty-guards-present',101,'APEX-CARDINALITY AccountObject.wallets expected/native length')
 start=original_test.index('fn collection_source_count(')
 end=original_test.index('fn collection_values(',start)
 test.write_text(original_test[:start]+'''fn collection_source_count(expected: &Value, name: &str) { let _ = (expected, name); }
fn collection_native_count(expected: &Value, native_len: usize, name: &str) { let _ = (expected, native_len, name); }
'''+original_test[end:])
 run('consuming-empty-length-guards-removed',101,'APEX-CARDINALITY AccountObject.wallets whole native value')
finally:
 source.write_text(original_source);test.write_text(original_test)
run('consuming-empty-restored',0,'test result: ok.')
