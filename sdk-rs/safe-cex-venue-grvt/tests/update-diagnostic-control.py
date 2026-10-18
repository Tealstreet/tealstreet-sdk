#!/usr/bin/env python3
"""Break raw update-key capture in the production member, compile RED, exact restore GREEN."""
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/binding/methods.rs'
original = source.read_bytes()
old = 'let field_count = a.get("update")?.own_keys().len();'
assert original.decode().count(old) == 1
command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-grvt', '--lib', 'constructed_update_counts_all_own_raw_keys_before_validation']
def run():
    result = subprocess.run(command, cwd=crate.parent, capture_output=True, text=True)
    return result, result.stdout + result.stderr
try:
    source.write_text(original.decode().replace(old, 'let field_count = 0;', 1))
    result, output = run()
    assert result.returncode == 101 and 'constructed_update_counts_all_own_raw_keys_before_validation ... FAILED' in output, output
    assert 'all raw keys must reach the source diagnostic' in output and 'could not compile' not in output, output
    print('production key capture: compiled named RED', flush=True)
finally:
    source.write_bytes(original)
    assert source.read_bytes() == original
result, output = run()
assert result.returncode == 0 and '1 passed; 0 failed' in output, output
print('exact restore: GREEN', flush=True)
