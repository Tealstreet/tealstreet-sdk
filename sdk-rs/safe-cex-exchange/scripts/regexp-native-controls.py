"""Replay the original RegExp coercion through the constructed native facade."""
import os
from pathlib import Path
import subprocess

package = Path(__file__).resolve().parents[1]
source = package / 'src/js_runtime/regexp.rs'
original = source.read_bytes()
fixed = b'    value.to_js_string()'
fault = b'    super::ops::to_primitive(value)?.to_js_string()'
assert original.count(fixed) == 1, 'review the changed RegExp string conversion'
command = [
    'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-exchange',
    '--all-features', '--test', 'js_builtins_native',
    'native_regexp_pending_inputs_match_node_at_the_public_facade', '--', '--exact',
]
environment = dict(os.environ, CARGO_INCREMENTAL='0')

def run():
    return subprocess.run(command, cwd=package.parent, env=environment,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

baseline = run()
assert baseline.returncode == 0 and '1 passed; 0 failed' in baseline.stdout, baseline.stdout
try:
    source.write_bytes(original.replace(fixed, fault, 1))
    broken = run()
    assert broken.returncode != 0, 'old number-hint coercion unexpectedly passed'
    assert 'native_regexp_pending_inputs_match_node_at_the_public_facade ... FAILED' in broken.stdout, broken.stdout
    assert 'stringBeforeNumber' in broken.stdout and 'units' in broken.stdout, broken.stdout
    print('original number-hint conversion: compiled named RED', flush=True)
finally:
    source.write_bytes(original)
restored = run()
assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, restored.stdout
assert source.read_bytes() == original, 'RegExp control did not restore exact source bytes'
print('exact source restoration: constructed native corpus GREEN', flush=True)
