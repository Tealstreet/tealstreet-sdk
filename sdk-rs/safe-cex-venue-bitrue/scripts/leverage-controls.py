"""One production mutation at a time, one Node process per complete corpus."""
from pathlib import Path
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/read_helpers.rs'
original = source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitrue', '--test', 'leverage', '--locked', '--jobs', '1']
controls = [
 ('setMarketMaxLeverage return', 'update.map_err(safe_cex_exchange::runtime::ExchangeError::from)', '{ let _ = update; Ok(()) }'),
 ('identity: fallback', 'update.map_err(safe_cex_exchange::runtime::ExchangeError::from)', 'update.map_err(|error| { let mut converted = safe_cex_exchange::runtime::ExchangeError::from(error); if converted.store_subscriber_ids.len() == 1 { converted.store_subscriber_ids.clear(); } converted })'),
 ('identity: recovered', 'update.map_err(safe_cex_exchange::runtime::ExchangeError::from)', 'update.map_err(|error| { let mut converted = safe_cex_exchange::runtime::ExchangeError::from(error); if converted.store_subscriber_ids.len() > 1 { converted.store_subscriber_ids.clear(); } converted })'),
 ('parseMaxLeverageFromErrorMessage', 'parsed.is_finite() && parsed > 0.0', 'parsed.is_finite() && parsed >= 0.0'),
 ('parseMaxLeverageFromBracketResponse', '.reduce(safe_cex_num::math_max)', '.reduce(safe_cex_num::math_min)'),
 ('setMarketMaxLeverage fields', 'limits.leverage.max = max_leverage;', 'limits.leverage.min = max_leverage;'),
 ('setMarketMaxLeverage update count', '(current_max - max_leverage).abs() < 1e-8', '(current_max - max_leverage).abs() < 1e-7'),
 ('setMarketMaxLeverage return', 'safe_cex_num::TableHit::Inherited(_)', 'safe_cex_num::TableHit::Missing'),
]
for name,old,new in controls:
 if len(sys.argv)>1 and old not in sys.argv[1:]: continue
 assert original.count(old)==1,old
 try:
  source.write_text(original.replace(old,new))
  result=subprocess.run(command,capture_output=True,text=True)
  output=result.stdout+result.stderr
  assertion=('leverage subscriber '+name) if name.startswith('identity:') else ('leverage native parity: '+name)
  assert result.returncode not in (0,126) and assertion in output,output
  print('FAIL PROVED: '+assertion,flush=True)
 finally:
  source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS: '+name,flush=True)
