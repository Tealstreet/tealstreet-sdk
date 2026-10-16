"""One compiling native assertion failure per new row, followed by restoration."""
from pathlib import Path
import os, subprocess
crate = Path(__file__).resolve().parent.parent
files = {name: crate / 'src' / name for name in ['fills.rs','retry.rs']}
originals = {name: path.read_bytes() for name,path in files.items()}
controls = [
 ('fills.rs',b'        price,\n        qty,',b'        price: qty,\n        qty: price,','fills::execution::distinct::route_0::price','fills'),
 ('fills.rs',b'        time,',b'        time: time / 1000.0,','fills::historical::distinct::route_0::time','fills'),
 ('retry.rs',b'60_000.0)',b'59_999.0)','retry::clamp::below_cap','retry'),
 ('retry.rs',b'72 | 104 => 3_600_000.0',b'72 | 104 => 36_000.0','retry::interval::hour','retry'),
]
command = [os.environ.get('CARGO','cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','2']
try:
 for name,before,after,label,target in controls:
  original = originals[name]
  assert original.count(before) == 1,(name,before)
  files[name].write_bytes(original.replace(before,after))
  result = subprocess.run(command+['--test',target],capture_output=True,text=True)
  output = result.stdout+result.stderr
  assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output,output
  print('COMPILED; exit101; '+label,flush=True)
  files[name].write_bytes(original)
finally:
 for name,path in files.items():path.write_bytes(originals[name])
subprocess.run(command+['--test','fills','--test','retry'],check=True)
print('RESTORED exit0; 4 new-row controls',flush=True)
