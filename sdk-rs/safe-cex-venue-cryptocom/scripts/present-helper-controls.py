"""Prove successful decoded values remain explicitly present in both helpers."""
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'safe-cex-venue-cryptocom/src/wire.rs'
def run(test):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom','--lib',test,'--','--exact'],cwd=ROOT,capture_output=True,text=True)
 return r.returncode,r.stdout+r.stderr
for fn,slug in [('deserialize_present_string','string'),('deserialize_present','generic')]:
 test=f'wire::accepted_present_helpers::present_{slug}_native_contract'
 code,output=run(test);assert code==0 and 'test result: ok. 1 passed' in output,output
 original=SOURCE.read_bytes();start=original.index(f'fn {fn}<'.encode());end=original.index(b'\n}\n',start)+3;region=original[start:end]
 assert region.count(b'.map(Some)')==1
 try:
  SOURCE.write_bytes(original[:start]+region.replace(b'.map(Some)',b'.map(|_| None)')+original[end:])
  code,output=run(test)
  assert code==101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output,output
  assert f'native present helper {slug}' in output,output
 finally:SOURCE.write_bytes(original)
 assert SOURCE.read_bytes()==original
 code,output=run(test);assert code==0 and 'test result: ok. 1 passed' in output,output
 print(f'{fn}: successful presence erased, compiled named runtime failure (exit 101); exact restoration PASS',flush=True)
