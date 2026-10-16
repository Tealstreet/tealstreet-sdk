"""One compiling native assertion control per newly mapped header helper."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/headers.rs'
original = path.read_bytes()
controls = [
    (b'let lower = (runtime.lowercase)(host, name);', b'let lower = (runtime.uppercase)(host, name);', 'headers::header::lookup_mixed::value'),
    (b'if (runtime.kind)(serialized) == Object {', b'if (runtime.kind)(serialized) == Function {', 'headers::entries::plain::len'),
    (b'if exhausted.is_empty() {', b'if !exhausted.is_empty() {', 'headers::delay::exhausted_priority::value'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'headers']
try:
    for before, after, label in controls:
        assert original.count(before) == 1, before
        path.write_bytes(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output, output
        print(f'COMPILED exit101 {label}', flush=True)
        path.write_bytes(original)
        subprocess.run(command, check=True)
        print(f'RESTORED exit0 {label}', flush=True)
finally:
    path.write_bytes(original)
print('3 new-row controls caught and individually restored', flush=True)
