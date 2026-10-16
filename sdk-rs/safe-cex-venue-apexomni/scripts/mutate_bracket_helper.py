"""Separate slippage and price-step controls against one actual-TS corpus per run."""
from pathlib import Path
import os, subprocess
root=Path(__file__).resolve().parents[3]
p=root/'sdk-rs/safe-cex-venue-apexomni/src/exchange_utils.rs'
original=p.read_text()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--locked','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--jobs','1','--test','utils','bracket_prices_match_actual_typescript','--','--nocapture']
logs=root/'sdk-rs/target/apexomni-controls'
controls=[('slippage','if side == "BUY" { 1.005 } else { 0.995 }','if side == "BUY" { 0.995 } else { 1.005 }'),
          ('step','safe_cex_num::adjust(execution_price, price_step)','safe_cex_num::adjust(execution_price, price_step.max(1.0))')]
try:
 for name,before,after in controls:
  assert original.count(before)==1,before
  p.write_text(original.replace(before,after))
  result=subprocess.run(command,cwd=root,capture_output=True,text=True)
  output=result.stdout+result.stderr
  (logs/f'bracket-{name}-mutation.log').write_text(output)
  assert result.returncode==101 and 'test result: FAILED.' in output,output
  assert 'calculateTpSlPrice native bits' in output,output
  print(name+': compiling mutation failed101 calculateTpSlPrice native bits',flush=True)
finally:p.write_text(original)
result=subprocess.run(command,cwd=root,capture_output=True,text=True)
(logs/'bracket-restored.log').write_text(result.stdout+result.stderr)
assert result.returncode==0,result.stdout+result.stderr
print('restored target: 0',flush=True)
