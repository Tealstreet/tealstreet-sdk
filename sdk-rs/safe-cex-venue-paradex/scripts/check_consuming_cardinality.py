#!/usr/bin/env python3
"""Venue-scoped consuming SeqAccess control; restores shared production bytes.
Run from the repository root, with no concurrent Cargo in this worktree.
Uses one Node subprocess for the full source corpus per target invocation.
"""
from pathlib import Path
import os
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-num/src/numeric_ingress.rs'
CARGO = os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo')
command = [CARGO, 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-paradex', '--test', 'types', '--jobs', '1',
           'native_collection_lengths_across_all_response_owners', '--', '--nocapture']
TEST = ROOT / 'sdk-rs/safe-cex-venue-paradex/tests/types.rs'
original_test = TEST.read_bytes()
original = SOURCE.read_bytes()
needle = b'''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
assert original.count(needle) == 1, 'consuming control production anchor drift'
replacement = b'''        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)'''
try:
    SOURCE.write_bytes(original.replace(needle, replacement))
    mutant = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    guard = b'if native[$field] != json!(2) {'
    assert original_test.count(guard) == 1, 'native guard anchor drift'
    # Keep the consuming mutant active and disable both source/native length
    # checks. The independently pinned executed field-count check stays enabled.
    bypass_test = original_test.replace(guard, b'if false {')
    bypass_test, removed = re.subn(
        rb'assert_eq!\(\s*case\["serialized"\]\[\$field\]\.as_array\(\)\.unwrap\(\)\.len\(\),\s*2,.*?\);',
        b'assert!(true);', bypass_test, flags=re.DOTALL)
    assert removed == 1, 'source length guard anchor drift'
    TEST.write_bytes(bypass_test)
    bypass = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
finally:
    SOURCE.write_bytes(original)
    TEST.write_bytes(original_test)
output = mutant.stdout + mutant.stderr
print(output, end='')
assert mutant.returncode == 101 and 'test result: FAILED' in output, 'mutant must compile and fail a test'
assert 'consuming_native_collection_cardinality' in output, 'named native assertion required'
for field in ['ORDER_RESPONSE.flags', 'BATCH_ORDER_RESPONSE.errors',
              'BATCH_ORDER_RESPONSE.orders', 'BATCH_CANCEL_ORDER_RESPONSE.results',
              'ORDERBOOK_RESPONSE.bids', 'ORDERBOOK_RESPONSE.asks', 'MARGIN_RESPONSE.configs']:
    assert field in output, f'no independent failure observed for {field}'
print(bypass.stdout + bypass.stderr, end='')
assert bypass.returncode == 0, 'guard bypass must expose the consuming mutant'
restored = subprocess.run(command, cwd=ROOT)
assert restored.returncode == 0, 'restored control must pass'
assert SOURCE.read_bytes() == original, 'shared source must be restored byte-for-byte'
print('consuming mutation: exit 101; seven named native fields; both length guards bypass: exit 0; restored: exit 0')
