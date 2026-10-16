"""Prove the Rust-side table oracle control; the test itself proves the TS side."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
root = crate.parents[1]
source = crate / 'src/types.rs'
original = source.read_text()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-mexc', '--test', 'tables', '--jobs', '2', '--', '--exact', 'mexc_tables_live_typescript_every_value', '--nocapture']
try:
    anchor = 'pub const RECV_WINDOW: u64 = 20_000;'
    assert original.count(anchor) == 1
    source.write_text(original.replace(anchor, 'pub const RECV_WINDOW: u64 = 20_001;'))
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 101, result.stdout
    assert 'mexc_tables_live_typescript_every_value ... FAILED' in result.stdout, result.stdout
    assert 'table RECV_WINDOW' in result.stdout, result.stdout
    print('Rust RECV_WINDOW mutation: named assertion failure, exit 101')
finally:
    source.write_text(original)
subprocess.run(command, cwd=root, check=True)
