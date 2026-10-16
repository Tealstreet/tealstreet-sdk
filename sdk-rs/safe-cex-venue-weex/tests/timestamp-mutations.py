"""Serial timestamp production mutations; one Node per whole corpus."""
from pathlib import Path
import os
import subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs'
original=source.read_text()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','timestamps','--','--nocapture']
controls=[('numeric / 1_000_000.0','numeric / 100_000.0','timestamp::normalizeTimestampToMs::nanoseconds::UTC::route_0'),('(milliseconds / 1000.0).floor()','(milliseconds / 1000.0).trunc()','timestamp::normalizeTimestampToSeconds::1969-12-31T23:59:59.999Z::UTC::route_0'),('js_date_parse(&js_string(value)?, local_to_utc)','js_date_parse(&js_string(value)?, |local| { let _ = local_to_utc(local); local })','timestamp::normalizeTimestampToMs::')]
try:
 for before,after,assertion in controls:
  assert original.count(before)==1,before
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode not in (0,126) and assertion in output and 'assertion `left == right` failed' in output,output
  print('FAIL observed: '+next(l for l in output.splitlines() if 'assertion `left == right` failed' in l),flush=True)
  source.write_text(original)
finally:source.write_text(original)
subprocess.run(command,check=True)
print('Restored PASS after 3 timestamp production mutations',flush=True)
