"""Sequential compiling controls for the private timestamp decoder."""
from pathlib import Path
import os
import subprocess

path = Path('sdk-rs/safe-cex-venue-woo/src/private_socket.rs')
original = path.read_text()
cases = [
    ('unit-threshold', 'parsed >= 1_000_000_000_000.0', 'parsed > 1_000_000_000_000.0', 'timestamp output bits'),
    ('floor', 'parsed.floor()', 'parsed', 'timestamp output bits'),
    ('zero', 'parsed <= 0.0', 'parsed < 0.0', 'timestamp absence'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, before, after, assertion in cases:
    assert before in original, name
    try:
        path.write_text(original.replace(before, after, 1))
        run = subprocess.run([
            'cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
            '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo',
            '--test', 'private_socket',
        ], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path(f'/tmp/woo-private-control-{name}.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)
