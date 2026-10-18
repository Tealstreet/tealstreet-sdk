#!/usr/bin/env python3
"""Require four compiled production faults to fail constructed socket tests."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-binance'
dispatch = crate / 'src/public_dispatch.rs'
capture = crate / 'src/ticker_capture.rs'
originals = {path: path.read_bytes() for path in [dispatch, capture]}
receipts = Path(tempfile.gettempdir()) / 'binance-public-dispatch-controls'
receipts.mkdir(exist_ok=True)
cases = [
    ('Handlers::get', dispatch,
     b'self.own\n            .borrow()\n            .get(key)',
     b'self.own\n            .borrow()\n            .get("__missing_control__")',
     'public_frame_guard::production_native_public_frame_guard_releases_success_error_and_unwind'),
    ('dispatch_binance_message', dispatch,
     b'broadcast.run(events.clone(), "broadcast")?;',
     b'let _ = events.clone();',
     'public_frame_guard::production_native_public_frame_guard_releases_success_error_and_unwind'),
    ('Capture::new', capture,
     b'if let Some(capture) = weak.upgrade() {\n                capture.refresh();\n            }',
     b'if let Some(_capture) = weak.upgrade() {}',
     'production_native_ticker_capture_aliases_retain_cells_and_replace_generations'),
    ('lookup', capture,
     b'.then_some(Target::StockInherited)',
     b'.then_some(Target::StockInherited).filter(|_| false)',
     'public_dispatch_contracts::production_native_dispatch_lookup_and_capture_follow_source_prototypes'),
]
requested = sys.argv[1:]
assert len(requested) <= 1 and all(name in {case[0] for case in cases} for name in requested)
if requested:
    cases = [case for case in cases if case[0] == requested[0]]


def run(test):
    result = subprocess.run([
        'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-binance',
        '--features', 'native', '--test', 'private_classes', test,
        '--', '--exact', '--nocapture',
    ], cwd=workspace, env=os.environ.copy(), capture_output=True, text=True, timeout=600)
    return result.returncode, result.stdout + result.stderr


try:
    for owner, path, before, after, test in cases:
        original = originals[path]
        assert original.count(before) == 1, owner
        path.write_bytes(original.replace(before, after))
        status, output = run(test)
        (receipts / f'{owner.replace("::", "-")}-fault.log').write_text(output)
        assert status != 0 and f'test {test} ... FAILED' in output and 'could not compile' not in output, output[-10000:]
        print(f'COMPILED RED: {owner} -> {test}', flush=True)
        path.write_bytes(original)
        status, output = run(test)
        (receipts / f'{owner.replace("::", "-")}-restore.log').write_text(output)
        assert status == 0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output, output[-10000:]
        print(f'EXACT RESTORE GREEN: {owner}', flush=True)
finally:
    for path, original in originals.items():
        path.write_bytes(original)
        assert path.read_bytes() == original
