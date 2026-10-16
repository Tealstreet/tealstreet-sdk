#!/usr/bin/env python3
"""Production and actual-source comparator controls; one Node per whole corpus."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'sort', '--jobs', '1', '--locked', '--', '--nocapture']
controls = [
    (crate / 'src/api.rs', 'collator.compare(a, b)', 'collator.compare(b, a)'),
    (sdk.parent / 'packages/safe-cex/src/exchanges/bingx/bingx.api.ts',
     'return a.localeCompare(b);', 'return b.localeCompare(a);'),
]
for path, before, after in controls:
    original = path.read_bytes()
    assert original.decode().count(before) == 1
    try:
        path.write_text(original.decode().replace(before, after))
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode not in (0, 126), result.stdout
        assert 'native sort ordering' in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'{path.name}: failed native sort ordering (exit {result.returncode})', flush=True)
    finally:
        path.write_bytes(original)
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
    print(f'{path.name}: restored 1 passed', flush=True)
