#!/usr/bin/env python3
"""Prove actual nested-helper execution and retain ordinary policy refusals."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'safe-cex-venue-hyperliquid'
SOURCE = CRATE / 'src/rate_limit.rs'
POLICY = ROOT / 'safe-cex-parity/accepted-set/local-functions/safe-cex-venue-hyperliquid.json'
KEY = 'safe-cex-venue-hyperliquid/src/rate_limit.rs::LOCAL_FUNCTION:text'
ORIGINAL = SOURCE.read_bytes()
ORIGINAL_POLICY = POLICY.read_bytes()
ENV = {**os.environ, 'RUSTC_BOOTSTRAP':'1', 'RUSTUP_TOOLCHAIN':'1.99.0', 'CARGO_BUILD_JOBS':'1'}
assert hashlib.sha256(ORIGINAL).hexdigest() == json.loads(ORIGINAL_POLICY)[KEY]['sourceSha256']
def native(mutation=False):
    command = [sys.executable, str(CRATE / 'scripts/accepted-rate-weight-native.py')]
    if mutation:
        command.append('--comma-only')
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if mutation:
        assert result.returncode == 101 and 'could not compile' not in result.stdout and not re.search(r'error\[E\d+\]', result.stdout)
        assert 'test rate_weight_contract_native_fields ... FAILED' in result.stdout
        assert 'rate text caller: comma join' in result.stdout
    else:
        assert result.returncode == 0 and 'test rate_weight_contract_native_fields ... ok' in result.stdout
try:
    needle = b"result.push(b',' as u16);"
    assert ORIGINAL.count(needle) == 1
    SOURCE.write_bytes(ORIGINAL.replace(needle, b"result.push(b'0' as u16);"))
    try:
        native(True)
    finally:
        SOURCE.write_bytes(ORIGINAL)
        assert SOURCE.read_bytes() == ORIGINAL
    native()
    old = json.loads(ORIGINAL_POLICY)
    old[KEY]['runtimeOwner'] = 'safe_cex_venue_hyperliquid::rate_limit::raw_number'
    POLICY.write_text(json.dumps(old, indent=2) + '\n')
    try:
        result = subprocess.run(['node', 'safe-cex-parity/scripts/accepted-set.mjs', '--package=safe-cex-venue-hyperliquid', '--jobs=1'], cwd=ROOT, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        print(result.stdout, flush=True)
        assert result.returncode != 0 and KEY + ': accepted-set assertion: decoder helper requires its inventoried production decoder' in result.stdout
    finally:
        POLICY.write_bytes(ORIGINAL_POLICY)
        assert POLICY.read_bytes() == ORIGINAL_POLICY
finally:
    SOURCE.write_bytes(ORIGINAL)
    POLICY.write_bytes(ORIGINAL_POLICY)
    assert SOURCE.read_bytes() == ORIGINAL and POLICY.read_bytes() == ORIGINAL_POLICY
print('Compiled nested comma mutation fails; source restoration passes51 native cases; old policy parent restores exact ordinary refusal; policy bytes restored.', flush=True)
