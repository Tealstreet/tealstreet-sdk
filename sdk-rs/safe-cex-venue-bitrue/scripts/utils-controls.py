from pathlib import Path
import subprocess,os
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/utils.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','utils','--jobs','1']
assert original.count("strip_prefix('E')")==1
try:
 source.write_text(original.replace("strip_prefix('E')","strip_prefix('Q')"))
 result=subprocess.run(command,capture_output=True,text=True)
 output=result.stdout+result.stderr
 assert result.returncode!=0 and 'reverseWsSymbol parity:' in output,output
 print('FAIL PROVED: reverseWsSymbol parity: "E"',flush=True)
finally:
 source.write_text(original)
result=subprocess.run(command,capture_output=True,text=True)
assert result.returncode==0,result.stdout+result.stderr
print('RESTORED PASS: reverseWsSymbol parity',flush=True)
