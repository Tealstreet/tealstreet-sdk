#!/usr/bin/env python3
"""Remove only Request's object guard, require compiled array admission, restore."""
from pathlib import Path
import subprocess
workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-candle-cache/src/owner.rs'
original = source.read_bytes()
old = b'RequestWire::deserialize(safe_cex_types::strict_decode::object(d))'
new = b'RequestWire::deserialize(d)'
assert original.count(old) == 1
command = ['cargo', 'test', '-p', 'safe-cex-candle-cache', '--bin', 'candle-cache-owner', '--jobs', '1']
def run():
    result = subprocess.run(command, cwd=workspace, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, end='', flush=True)
    return result
try:
    source.write_bytes(original.replace(old, new))
    result = run()
    assert result.returncode == 101
    assert 'test host::owner_request_preserves_fields_and_refuses_complete_array ... FAILED' in result.stdout
    assert 'complete ordered array must be refused by raw request decoder' in result.stdout
    source.write_bytes(original)
    assert run().returncode == 0
finally:
    source.write_bytes(original)
assert source.read_bytes() == original
print('Compiled complete-array control failed the named assertion; exact restoration passed.')
