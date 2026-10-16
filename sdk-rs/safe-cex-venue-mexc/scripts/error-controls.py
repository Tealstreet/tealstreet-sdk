"""Incorrect strict-success and eager formatting paths must fail by name."""
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/exchange_errors.rs';original=source.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-venue-mexc','--test','exchange_errors','--jobs','2','--','--exact','mexc_exchange_errors_live_typescript','--nocapture']
controls=[('strict success','!= Some(&Value::Bool(false))','== Some(&Value::Bool(true))','assertCommonSuccess'),('eager row coercion','if to_number(row.get("errorCode"), Some(0.0)).map_err(coercion)? != 0.0 {','let _ = to_integer_string(row.get("orderId")).map_err(coercion)?;\n        if to_number(row.get("errorCode"), Some(0.0)).map_err(coercion)? != 0.0 {','assertRegularCancelSucceeded')]
try:
 for name,before,after,witness in controls:
  assert original.count(before)==1,(name,original.count(before))
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,cwd=crate.parents[1],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101,result.stdout
  assert 'mexc_exchange_errors_live_typescript ... FAILED' in result.stdout,result.stdout
  assert witness in result.stdout and 'assertion `left == right` failed' in result.stdout,result.stdout
  print(f'Rust {name}: compiled then named assertion fails at 101',flush=True)
finally:source.write_text(original)
subprocess.run(command,cwd=crate.parents[1],check=True)
