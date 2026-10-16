"""Sequential production controls: one Node process for the whole account corpus."""
import os
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/account.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','account','--locked','--jobs','1']
controls=[
 ('getAccount currency', '"currency": "USDT"', '"currency": "USDC"', 'account request and emit order'),
 ('getAccount native user ID', 'user_id: js_string(Some(user_id))?', 'user_id: format!("wrong-{}", js_string(Some(user_id))?)', 'native account userId'),
 ('getAccount emitter propagation', 'emit_and_log_error(message)?;', 'let _ = emit_and_log_error(message);', 'account error name'),
]
for name,before,after,assertion in controls:
 try:
  assert original.count(before)==1,(name,before)
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True)
  assert result.returncode not in (0,126) and assertion in result.stdout+result.stderr,result.stdout+result.stderr
  print(f'FAIL confirmed: {name}: {assertion}',flush=True)
 finally: source.write_text(original)
 restored=subprocess.run(command,capture_output=True,text=True)
 assert restored.returncode==0,restored.stdout+restored.stderr
 print(f'PASS restored: {name}',flush=True)
