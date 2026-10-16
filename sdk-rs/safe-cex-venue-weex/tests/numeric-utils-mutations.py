"""Three production controls, serial cargo and one Node per complete corpus."""
from pathlib import Path
import os
import subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs'
original=source.read_text()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','numeric_utils','--','--nocapture']
controls=[('toNumber','if parsed.is_finite() { parsed } else { fallback }','if parsed.is_finite() { parsed.abs() } else { fallback }'),('toIntegerString','return Ok(String::new());','return Ok("undefined".to_owned());'),('normalizePercentage','parsed * 100.0','parsed * 10.0')]
# Restrict each mutation to its own source function; formatting is part of the census.
functions=['to_number','to_integer_string','normalize_percentage']
try:
 for (name,before,after),function in zip(controls,functions):
  start=original.index('pub fn '+function+'(');end=original.index('\n}',start)+2
  block=original[start:end]
  assert block.count(before)==1,(name,block)
  source.write_text(original[:start]+block.replace(before,after)+original[end:])
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode not in (0,126) and 'numeric_utility::'+name+'::' in output and 'assertion `left == right` failed' in output,output
  print('FAIL observed: '+next(line for line in output.splitlines() if 'assertion `left == right` failed' in line),flush=True)
  source.write_text(original)
finally:source.write_text(original)
subprocess.run(command,check=True)
print('Restored PASS after 3 numeric utility production mutations',flush=True)
