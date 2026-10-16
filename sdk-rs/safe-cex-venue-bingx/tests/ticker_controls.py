#!/usr/bin/env python3
"""Separate native bid/ask production precision substitutions; fixtures unchanged."""
import hashlib
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/ticker.rs'
oracle = crate / 'tests/ticker_oracle.cjs'
original_oracle = oracle.read_bytes()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'ticker', '--jobs', '1', '--locked', '--', '--nocapture']
for field, before, after in [
    ('bid', 'subtract(last, price_step)', 'subtract(last, precision.map_or(0.0, |p| p.amount))'),
    ('ask', 'add(last, price_step)', 'add(last, precision.map_or(0.0, |p| p.amount))'),
]:
    original = source.read_bytes()
    assert original.decode().count(before) == 1
    assertion = f'BINGX-PRECISION native {field}'
    try:
        source.write_text(original.decode().replace(before, after))
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode == 101, result.stdout
        assert assertion in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(f'{field}: compiled; exit101 at {assertion}', flush=True)
    finally:
        source.write_bytes(original)
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
    assert oracle.read_bytes() == original_oracle, 'oracle remains byte-identical'
    print(f'{field}: restored 1 passed; oracle sha256 {hashlib.sha256(original_oracle).hexdigest()}', flush=True)
