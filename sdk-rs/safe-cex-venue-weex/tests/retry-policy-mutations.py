"""One compiling native-result control per new retry-policy row; run serially."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/retry.rs';original=source.read_bytes()
controls=[
 (b'seconds * 1_000.0',b'seconds * 100.0','retry_policy::retry_after::seconds'),
 (b'Some("GET" | "HEAD") => true',b'Some("HEAD") => true','retry_policy::read_only::get'),
 (b'retry_count.unwrap_or(0.0) < 1.0',b'retry_count.unwrap_or(0.0) < 2.0','retry_policy::policy::rate_second'),
]
command=[os.environ.get('CARGO','cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','2','--test','retry_policy']
try:
 for before,after,label in controls:
  assert original.count(before)==1,before
  source.write_bytes(original.replace(before,after))
  r=subprocess.run(command,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and label in out and 'assertion `left == right` failed' in out,out
  print('COMPILED exit101 '+label,flush=True)
  source.write_bytes(original)
  subprocess.run(command,check=True)
  print('RESTORED exit0 '+label,flush=True)
finally:source.write_bytes(original)
print('3 new-row controls caught and individually restored',flush=True)
