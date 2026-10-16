"""One compiling native-result mutation per newly ported websocket control row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/ws_control.rs'
original = path.read_bytes()
controls = [
    (b'"Tealstreet".encode_utf16()', b'"tealstreet".encode_utf16()', 'auth/basic::header_value'),
    (b'"positions", "orders", "fill"', b'"positions", "orders", "fills"', 'subscribe/basic::params'),
    (b'state.has_logged_unsupported = true;', b'state.has_logged_unsupported = false;', 'log/default::logged'),
    (b'(runtime.snapshot)(host, state, 10.0)?;', b'(runtime.snapshot)(host, state, 11.0)?;', 'private_open/basic::snapshot_limit'),
]
for name, end in [('private_ping', 'public_ping'), ('public_ping', 'public_on_open')]:
    start = original.index(('pub fn ' + name + '<').encode())
    stop = original.index(('pub fn ' + end + '<').encode(), start)
    before = original[start:stop]
    assert before.count(b'    ping(host, state, runtime)\n') == 1
    after = before.replace(b'    ping(host, state, runtime)\n', b'    if state.is_disposed { ping(host, state, runtime) } else { Ok(()) }\n')
    controls.append((before, after, name + '/basic::id'))
controls.append((b'state.has_logged_limit = false;', b'state.has_logged_limit = true;', 'public_open/basic::logged'))
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'ws_control']
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
print('7 controls caught and individually restored', flush=True)
