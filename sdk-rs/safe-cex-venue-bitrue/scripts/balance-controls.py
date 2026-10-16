"""Sequential production controls, one Node subprocess per complete corpus."""
from pathlib import Path
import subprocess, os
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/balance.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','balance','--locked','--jobs','1']
controls=[
 ('native field swap','                total,\n                free,','                total: free,\n                free: total,'),
 ('primary priority','.get("USDT")\n        .or_else(|| currencies.get("USDC"))','.get("USDC")\n        .or_else(|| currencies.get("USDT"))'),
 ('maintenance candidate','            maintenance,','            maintenance * 0.0,'),
 ('negative candidate','            total - free,','            total + free,'),
]
for label,old,new in controls:
 assert original.count(old)==1,old
 try:
  source.write_text(original.replace(old,new))
  result=subprocess.run(command,capture_output=True,text=True)
  output=result.stdout+result.stderr
  assert result.returncode==101 and 'balance native parity: parseBalance' in output and 'could not compile' not in output,output
  print('FAIL 101 PROVED: balance native parity: parseBalance ('+label+')',flush=True)
 finally:source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS: '+label,flush=True)
