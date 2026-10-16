"""Suppress the supplemental ticker assertion block; require its hit-count failure."""
from pathlib import Path
import os, subprocess
crate=Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
path=crate/'tests/ticker_reads.rs'
original=path.read_text()
old='raw.contains("\\\"error\\\":null")'
assert original.count(old)==1
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','ticker_reads','--locked','--jobs','1']
try:
 path.write_text(original.replace(old,'raw.is_empty() && '+old))
 result=subprocess.run(command,capture_output=True,text=True)
 evidence=result.stdout+result.stderr
 assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and 'PHEMEX-GATED ticker raw lexeme comparison count' in evidence,evidence
 print('COMPILED FAIL101: PHEMEX-GATED ticker raw lexeme comparison count',flush=True)
finally:path.write_text(original)
result=subprocess.run(command,capture_output=True,text=True)
assert result.returncode==0,result.stdout+result.stderr
print('PASS restored ticker guard',flush=True)
