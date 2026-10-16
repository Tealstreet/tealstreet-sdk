"""A compiling production mutation per newly ported selection row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/selection.rs'
original = path.read_bytes()
controls = [
    (b'return Ok(payload);', b'return Ok((runtime.empty_array)(host));', 'selection::array::payload_array'),
    (b'return Ok(false);', b'return Ok(true);', 'selection::runtime::absent'),
    (b'100_usize.saturating_sub(topic_count)', b'100_usize.saturating_add(topic_count)', 'selection::topics::capacity_11'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'selection']
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
print('3 controls caught and individually restored', flush=True)
