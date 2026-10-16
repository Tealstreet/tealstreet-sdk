"""Independent plausible Rust mapper mutations; named exit-101 assertions."""
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/utils.rs'
original=source.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-venue-mexc','--test','mappers','--jobs','2','--','--exact','mexc_mappers_live_typescript_every_value','--nocapture']
controls=[
 ('normalizePlanOrder','let status = normalize_trigger_order_status(raw)?;', 'let status = numeric_table(&crate::types::ORDER_STATUS, to_number(pick(raw, &["state", "status"]), Some(f64::NAN))?)?.unwrap_or(OrderStatus::Open);'),
 ('normalizeStopOrder','let amount = if full_position {','let amount = if false {'),
 ('normalizeOrder','order.remaining = math_max(amount - filled, 0.0);','order.remaining = amount - filled;'),
 ('normalizePosition','notional / leverage','notional'),
]
try:
 for name,before,after in controls:
  assert original.count(before)==1,(name,original.count(before))
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,cwd=crate.parents[1],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101,result.stdout
  assert 'mexc_mappers_live_typescript_every_value ... FAILED' in result.stdout,result.stdout
  assert name in result.stdout and 'assertion `left == right` failed' in result.stdout,result.stdout
  print(f'Rust {name}: named exit 101',flush=True)
finally:source.write_text(original)
subprocess.run(command,cwd=crate.parents[1],check=True)
