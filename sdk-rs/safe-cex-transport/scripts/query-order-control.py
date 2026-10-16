#!/usr/bin/env python3
"""Measure redundant HOW ordering and remove all frame ordering for a negative control."""
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[3]
p=root/'sdk-rs/safe-cex-transport/src/how.rs'
original=p.read_bytes();text=original.decode()
command=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-transport','--jobs','2','--test','tunnel','emitted_how_frame_uses_javascript_integer_property_order','--','--exact']
needle='normalize_object_order(&mut params);'
assert text.count(needle)==1

def run(label, changed, expected):
 try:
  p.write_text(changed)
  result=subprocess.run(command,cwd=root,capture_output=True,text=True)
  output=result.stdout+result.stderr
  Path('/tmp/core-p4a-'+label+'.log').write_text(output)
  if result.returncode != expected or 'error[E' in output or (expected==101 and 'HOW integer query property order' not in output):
   raise SystemExit(label+': unexpected result\n'+output)
  print(f'{label}: exit {result.returncode}',flush=True)
 finally:p.write_bytes(original)
 restored=subprocess.run(command,cwd=root,capture_output=True,text=True)
 Path('/tmp/core-p4a-'+label+'-restored.log').write_text(restored.stdout+restored.stderr)
 if restored.returncode:raise SystemExit(restored.stdout+restored.stderr)
 print(label+': restored exit 0',flush=True)

# Keep this survivor visible: JS stringify also orders keys at HOWWebSocket::send.
run('top-level-order-survivor',text.replace(needle,''),0)
start=text.index('fn normalize_object_order(');end=text.index('\npub(crate) fn normalize_json_numbers',start)
mutated=text[:start]+'fn normalize_object_order(_values: &mut serde_json::Map<String, Value>) {}\n'+text[end:]
start=mutated.index('        let message =',mutated.index('pub fn send(&self, data: Value)'))
end=mutated.index('        let socket =',start)
# Bypass the redundant final ECMAScript stringify ordering as well as HOW normalization.
mutated=mutated[:start]+'''        let message = serde_json::to_string(&data)
            .map_err(|error| TransportError::Host(error.to_string()))?;
'''+mutated[end:]
run('all-frame-order-normalization-removed',mutated,101)
