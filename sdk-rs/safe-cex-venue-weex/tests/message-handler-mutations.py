"""A compiling production mutation per newly ported message-handler row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/message_handlers.rs'
original = path.read_bytes()
controls = [
    (b'    handlers.push(handler);', b'    handlers.insert(0, handler);', 'handlers::add/append::state_value'),
    (b'        !(runtime.strict_equal)(item, handler)', b'        (runtime.strict_equal)(item, handler)', 'handlers::remove/all_duplicates::state_value'),
    (b'    let mut iterator = (runtime.iterator)(host, &handlers)?;', b'    if (runtime.truthy)(&handlers) { return Ok(()); }\n    let mut iterator = (runtime.iterator)(host, &handlers)?;', 'handlers::route/ordered::call_count'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'message_handlers']
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
