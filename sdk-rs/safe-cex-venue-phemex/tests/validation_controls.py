"""Production mutations; one live Node corpus per target and one Cargo at a time."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
source=crate/'src/validation.rs';original=source.read_text()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','validation','--locked','--jobs','1']
controls=[
 ('total field swap','v.get("accountBalanceRv")','v.get("totalUsedBalanceRv")','native balance.total'),
 ('free reversal','free: total - used,','free: used - total,','native balance.free'),
 ('nonfinite total admitted','if !total.is_finite()','if total.is_nan()','native balance presence'),
 ('negative zero survives fallback','used.is_nan() || used == 0.0','used.is_nan()','native balance.used'),
 ('returned message swap','data.and_then(|v| v.get("msg"))','data.and_then(|v| v.get("message"))','native error'),
 ('emitter failure swallowed','emit_and_log_error(message)?;','let _ = emit_and_log_error(message);','outcome'),
]
for kind in ['Array','Object']:
 for count,label in [(0,'empty'),(1,'subset')]:
  replacement=f'AccountValidationError::from_value(match error {{ Value::{kind}(rows) => Value::{kind}(rows.into_iter().take({count}).collect()), other => other }})'
  controls.append((f'error {kind} {label}','AccountValidationError::from_value(error)',replacement,'native error'))
 extra='rows.push(Value::Null);' if kind=='Array' else 'rows.insert("extra".into(), Value::Null);'
 replacement=f'AccountValidationError::from_value(match error {{ Value::{kind}(mut rows) => {{ {extra} Value::{kind}(rows) }}, other => other }})'
 controls.append((f'error {kind} extra','AccountValidationError::from_value(error)',replacement,'native error'))
assert len(controls)==12,'validation control inventory'
completed=0
for name,before,after,assertion in controls:
 try:
  assert original.count(before)==1,(name,before)
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True)
  evidence=result.stdout+result.stderr
  assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and 'PHEMEX-VALIDATION '+assertion in evidence,(name,evidence)
  print(f'COMPILED FAIL101: {name}: PHEMEX-VALIDATION {assertion}',flush=True)
 finally:source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('PASS restored: '+name,flush=True)
 completed+=1
assert completed==12,'validation executed control inventory'
