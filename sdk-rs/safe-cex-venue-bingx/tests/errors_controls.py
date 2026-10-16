#!/usr/bin/env python3
"""Named production mutations with restoration; one Node invocation per corpus."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/errors.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'errors', '--jobs', '1', '--locked', '--', '--nocapture']
for before, after, assertion in [
    (' (BingX message: {message})', ' (BingX message: wrong {message})', 'native buildTradfiOneWayModeError'),
    ('if error_message', 'if !error_message', 'native normalizeOrderPlacementError'),
]:
    original = source.read_bytes()
    assert original.decode().count(before) == 1
    try:
        source.write_text(original.decode().replace(before, after))
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode not in (0, 126), result.stdout
        assert assertion in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'failed {assertion} (exit {result.returncode})', flush=True)
    finally:
        source.write_bytes(original)
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
    print('restored 1 passed', flush=True)
