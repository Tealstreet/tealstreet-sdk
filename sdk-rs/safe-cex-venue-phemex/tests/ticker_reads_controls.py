"""One Node process per corpus; sequential production fail/restore controls."""
import os
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/ticker_reads.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','ticker_reads','--locked','--jobs','1']
controls=[
 ('ticker null fallback','return Ok(Value::Null);','return Ok(Value::Bool(false));','native ticker result'),
 ('ticker endpoint','*name == "TICKERS"','*name == "ACCOUNT"','ticker request and emit order'),
 ('ticker emitter propagation','emit_and_log_error(error_field(data.as_ref())?)?;','let _ = emit_and_log_error(error_field(data.as_ref())?);','ticker outcome'),
]
for name,before,after,assertion in controls:
 try:
  assert original.count(before)==1,(name,before)
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True)
  assert result.returncode==101 and assertion in result.stdout+result.stderr,result.stdout+result.stderr
  print(f'FAIL 101 confirmed: {name}: {assertion}',flush=True)
 finally: source.write_text(original)
 restored=subprocess.run(command,capture_output=True,text=True)
 assert restored.returncode==0,restored.stdout+restored.stderr
 print(f'PASS restored: {name}',flush=True)
