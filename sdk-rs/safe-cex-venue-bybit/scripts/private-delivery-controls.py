#!/usr/bin/env python3
"""Compiled receiver regressions; restores exact bytes and stores output only in /tmp."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
protocol = root / 'sdk-rs/safe-cex-venue-bybit/src/ws_private/protocol.rs'
messages = root / 'sdk-rs/safe-cex-venue-bybit/src/ws_private/messages.rs'
original = {p:p.read_bytes() for p in [protocol,messages]}
command = ['cargo','test','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','private_event_delivery','--','--include-ignored']
controls = [
 ('unwired',protocol,'        self.message(socket, frame)','        if socket.is_disposed() { self.message(socket, frame) } else { Err(ExchangeError::new("Not implemented")) }','production_private_frames_publish_orders_incremental_fills_and_terminal_statuses'),
 ('category',messages,'if this.call_method("tradesCategoryOf", vec![symbol])?.truthy() {','if { let _ = this.call_method("tradesCategoryOf", vec![symbol])?; true } {','production_position_frames_update_positions_balance_and_filter_untraded_categories'),
 ('fill-cache',messages,'(self.fills.borrow_mut())(', '(self.fills.replace(Box::new(safe_cex_store::incremental_fills::create_incremental_fills(500.))))(', 'production_private_frames_publish_orders_incremental_fills_and_terminal_statuses'),
 ('mark-seen',messages,'this.call_method("markOrderSeen", vec![mapped.get("id")?])?;','let _ = mapped.get("id")?;', 'private_order_sighting_clears_the_recent_placement_snapshot_grace'),
 ('position',messages,'vec![Js::array(positions)]','vec![Js::array(vec![])]','production_position_frames_update_positions_balance_and_filter_untraded_categories'),
 ('auth-limit',messages,'self.auth_retries.get() <= 3','self.auth_retries.get() < 3','private_auth_rejections_retry_four_times_without_inventing_wallet_or_execution_subscriptions'),
]
def run(label):
 r = subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/bybit27-control-'+label+'.txt').write_text(r.stdout)
 return r
baseline=run('baseline')
assert baseline.returncode==0 and '6 passed; 0 failed' in baseline.stdout,baseline.stdout
try:
 for label,p,before,after,test in controls:
  source=original[p].decode(); assert source.count(before)==1,(label,source.count(before))
  p.write_text(source.replace(before,after))
  result=run(label)
  assert result.returncode==101 and 'panicked at' in result.stdout and test+' ... FAILED' in result.stdout and 'error[E' not in result.stdout,(label,result.stdout)
  p.write_bytes(original[p])
  print(label+': compiled named runtime assertion failed',flush=True)
finally:
 for p,data in original.items():p.write_bytes(data)
restored=run('restored')
assert restored.returncode==0 and '6 passed; 0 failed' in restored.stdout,restored.stdout
assert all(p.read_bytes()==data for p,data in original.items())
print('6 controls caught; byte-identical restoration; 6 E2Es/source tests pass.',flush=True)
