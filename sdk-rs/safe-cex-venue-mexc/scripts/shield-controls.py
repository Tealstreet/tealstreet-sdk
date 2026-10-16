"""Plausible shield expiry and cancellation mistakes must fail the live trace."""
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/exchange_helpers.rs';original=source.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-venue-mexc','--test','shields','--jobs','2','--','--exact','mexc_shields_live_typescript_state_transitions','--nocapture']
controls=[('inclusive expiry','now() - at > ttl','now() - at >= ttl','isRecentlyPlacedStopLikeOrder'),('cancel retains placement','canceled.insert(id.into(), timestamp);\n        placed.remove(id);','canceled.insert(id.into(), timestamp);\n        let _ = &placed;','noteRecentlyCanceledStopLikeOrders'),('empty cancellation query','!order_id.is_empty() && shield_contains(canceled, order_id, now, 15_000.0)','shield_contains(canceled, order_id, now, 15_000.0)','isRecentlyCanceledStopLikeOrderId')]
controls.append(('frozen filter clock', ') -> Vec<Order> {\n    orders', ') -> Vec<Order> {\n    let timestamp = now();\n    let frozen = || timestamp;\n    let now: &dyn Fn() -> f64 = &frozen;\n    orders', 'filterRecentlyCanceledStopLikeOrders'))
try:
 for name,before,after,witness in controls:
  assert original.count(before)==1,(name,original.count(before))
  source.write_text(original.replace(before,after))
  result=subprocess.run(command,cwd=crate.parents[1],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101,result.stdout
  assert 'mexc_shields_live_typescript_state_transitions ... FAILED' in result.stdout,result.stdout
  assert witness in result.stdout and 'assertion `left == right` failed' in result.stdout,result.stdout
  print(f'Rust {name}: compiled then named assertion fails at 101',flush=True)
finally:source.write_text(original)
subprocess.run(command,cwd=crate.parents[1],check=True)
