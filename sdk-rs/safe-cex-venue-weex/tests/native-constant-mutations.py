"""Swap production native variants in all fifteen enum-bearing constant tables."""
from pathlib import Path
import os,re,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/types.rs';original=source.read_text()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','types_oracle']
blocks=[b for b in re.finditer(r'pub const (\w+):[\s\S]*?;\n',original) if re.search(r'\w+::\w+',b[0])]
assert len(blocks)==15
try:
 for block in blocks:
  variants=list(dict.fromkeys(re.findall(r'\w+::\w+',block[0])));a,b=variants[:2]
  changed=block[0].replace(a,'__SWAP__').replace(b,a).replace('__SWAP__',b)
  source.write_text(original[:block.start()]+changed+original[block.end():])
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assertion='constant::'+block[1]+'::native'
  assert result.returncode not in (0,126) and assertion in output and 'assertion `left == right` failed' in output,output
  print('FAIL observed: '+assertion,flush=True)
  source.write_text(original)
finally:source.write_text(original)
subprocess.run(command,check=True)
print('Restored PASS after 15 native enum production swaps',flush=True)
