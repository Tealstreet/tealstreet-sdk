"""Compiled provider controls, each with exact-byte restoration and named failure."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[3]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-registry', '--test', 'public_data_providers', '--jobs', '2']
controls = [
    ('filtered-liveness', 'in_process_public_data.rs', 'provider.note_data_observed();', '', 'PUBLIC native liveness before filter'),
    ('ping-threshold', 'external_public_data.rs', 'elapsed > EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS', 'elapsed >= EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS', 'PUBLIC native ping exact threshold excluded'),
    ('reconnect-reset', 'external_public_data.rs', 'self.reconnect_count.set(0);', '', 'PUBLIC native reconnect reset allows each turn'),
]
for name, filename, old, new, assertion in controls:
    source = root / 'sdk-rs/safe-cex-registry/src' / filename
    original = source.read_bytes()
    text = original.decode()
    if text.count(old) != 1:
        raise SystemExit(name + ': mutation site drift')
    try:
        source.write_text(text.replace(old, new))
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != 101 or assertion not in output or 'error[E' in output:
            raise SystemExit(name + ': missing compiled named failure (survivor/finding or compile error)\n' + output)
        print(f'{name}: exit 101 at {assertion}', flush=True)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
    if restored.returncode:
        raise SystemExit(restored.stdout + restored.stderr)
    print(f'{name}: restored exit 0 (5 tests)', flush=True)
