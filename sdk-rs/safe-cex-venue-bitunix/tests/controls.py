"""Sequential production mutations; always restore, never rewrite source fixtures."""
import os
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitunix/src/types.rs'
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-bitunix', '--jobs', '1', '--test', 'constants', '--', '--nocapture']
logs = root / 'sdk-rs/target/bitunix-controls'
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
# Mutate an observable value in each production declaration independently.
blocks = list(re.finditer(r'pub const (\w+):.*?(?=\npub const |\Z)', original, re.S))
try:
    for block in blocks:
        name = block[1]
        changed = block[0]
        if name == 'RECV_WINDOW':
            changed = changed.replace('20_000.0', '20_001.0')
        else:
            changed = re.sub(r'"([^"]+)"', lambda m: '"' + m[1] + '_BROKEN"', changed, count=1)
        assert changed != block[0], name
        source.write_text(original[:block.start()] + changed + original[block.end():])
        result = subprocess.run(command, text=True, capture_output=True)
        (logs / f'constant-{name}-fail.log').write_text(result.stdout + result.stderr)
        assertion = f'constant witness {name}'
        assert result.returncode != 0 and assertion in result.stdout + result.stderr, (name, result.stdout, result.stderr)
        print(f'FAIL observed: {assertion}', flush=True)
        source.write_text(original)
        restored = subprocess.run(command, text=True, capture_output=True)
        (logs / f'constant-{name}-pass.log').write_text(restored.stdout + restored.stderr)
        assert restored.returncode == 0, restored.stdout + restored.stderr
        print(f'PASS restored: {assertion}', flush=True)
finally:
    source.write_text(original)
