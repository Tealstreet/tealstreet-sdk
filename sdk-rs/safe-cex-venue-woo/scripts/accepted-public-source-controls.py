from pathlib import Path
import os, subprocess, hashlib, tempfile

# Serial native controls save and always restore the actual candidate source.
# Run without another Cargo process.

root=Path(__file__).resolve().parents[3];source=root/'sdk-rs/safe-cex-venue-woo/src/v3_public.rs';original=source.read_bytes()
env={**os.environ,'RUSTC_BOOTSTRAP':'1','CARGO_BUILD_JOBS':'1'}
output=Path(tempfile.mkdtemp(prefix='woo-public-controls-'))
print('Native source-control receipts:', output, flush=True)
command=['cargo','test','--locked','-p','safe-cex-venue-woo','--jobs=1','--test','v3_public','strict_public_decoders_and_mappers_match_live_typescript','--','--exact']
mutations=[('market-minimum','                value\n            };\n            let amount_max =','                value * 2.0\n            };\n            let amount_max ='),('candle-order','right[r].timestamp - left[l].timestamp < 0.0','right[r].timestamp - left[l].timestamp > 0.0')]
try:
 for label,before,after in mutations:
  text=original.decode();assert text.count(before)==1,label;source.write_text(text.replace(before,after))
  result=subprocess.run(command,cwd=root/'sdk-rs',env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (output/(label+'.log')).write_text(result.stdout)
  assert result.returncode!=0 and 'Finished `test` profile' in result.stdout and 'test strict_public_decoders_and_mappers_match_live_typescript ... FAILED' in result.stdout,(label,result.returncode,result.stdout[-1200:])
  print(label,'COMPILED -> named native FAIL',flush=True)
  source.write_bytes(original)
  restored=subprocess.run(command,cwd=root/'sdk-rs',env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (output/(label+'-restored.log')).write_text(restored.stdout)
  assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,(label,restored.stdout[-1200:])
  print(label,'exact restoration native PASS',flush=True)
finally:
 source.write_bytes(original)
 assert hashlib.sha256(source.read_bytes()).digest()==hashlib.sha256(original).digest()
