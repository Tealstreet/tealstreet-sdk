from pathlib import Path
import subprocess, os
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/account_helpers.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','accounts','--locked','--jobs','1']
controls=[
 ('getPositionAccounts','Some("USDT" | "USDC")','Some("USDT" | "BTC")'),
 ('getPositionAccounts','["volume", "openPrice", "leverageLevel"]','["openPrice", "volume", "leverageLevel"]'),
 ('getAccountPositionSnapshots','leverage_level: &position["leverageLevel"],\n                    position_type: &position["positionType"],','leverage_level: &position["positionType"],\n                    position_type: &position["leverageLevel"],'),
]
for name,old,new in controls:
 assert original.count(old)==1,old
 try:
  source.write_text(original.replace(old,new))
  result=subprocess.run(command,capture_output=True,text=True)
  output=result.stdout+result.stderr
  assertion='account native parity: '+name
  assert result.returncode not in (0,126) and assertion in output,output
  print('FAIL PROVED: '+assertion,flush=True)
 finally:
  source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS: '+name,flush=True)
