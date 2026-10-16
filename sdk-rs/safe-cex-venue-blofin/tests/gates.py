"""Package-scoped acceptance; every Cargo invocation uses one job."""
import os
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[3]
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
commands = [
    ('fmt', [cargo, 'fmt', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--check']),
    ('test', [cargo, 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--all-targets', '--', '--nocapture']),
    ('clippy-host', [cargo, 'clippy', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--all-targets', '--', '-D', 'warnings']),
    ('clippy-wasm', [cargo, 'clippy', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--all-targets', '--target', 'wasm32-unknown-unknown', '--', '-D', 'warnings']),
    ('parity', [sys.executable, str(root / 'sdk-rs/safe-cex-venue-blofin/tests/parity_scope.py')]),
]
logs = root / 'sdk-rs/target/blofin-gates'
logs.mkdir(exist_ok=True)
for name, command in commands:
    print(f'RUN {name}', flush=True)
    with (logs / f'{name}.log').open('w') as out:
        result = subprocess.run(command, cwd=root, stdout=out, stderr=subprocess.STDOUT)
    if result.returncode:
        print((logs / f'{name}.log').read_text(), flush=True)
        raise SystemExit(result.returncode)
    print(f'PASS {name}', flush=True)
