#!/usr/bin/env python3
"""Five family controls plus three concrete review regressions, sequentially."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/local_order_cache.rs'
evidence = root / 'sdk-rs/safe-cex-venue-bybit/evidence/local-order-cache-controls.json'
original = source.read_bytes()
gate_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--test', 'local_order_cache', '--jobs', '2', '--', '--include-ignored']
family_controls = [
    ('site predicate', '== Some(crate::site::BYBIT_BRAZIL_SITE_ID)', '!= Some(crate::site::BYBIT_BRAZIL_SITE_ID)', 'CACHE native fallback'),
    ('seen deletion', 'id != order_id', 'id == order_id', 'CACHE native cache count'),
    ('record guard', '== OrderType::Market', '== OrderType::Limit', 'CACHE native cache count'),
    ('cancel retirement', 'mark_order_seen(cache, id);', 'mark_order_seen(cache, if id == "a" { "wrong-id" } else { id });', 'CACHE native cache count'),
    ('strict grace expiry', '> 8_000.', '>= 8_000.', 'CACHE native preserved count'),
]
controls = [(name, [(before, after, 1)], assertion) for name, before, after, assertion in family_controls]
controls += [
    ('site read after clock', [
        ('if should_use_local_rest_order_lifecycle_fallback(site_id().as_ref()) {', 'if should_use_local_rest_order_lifecycle_fallback(eager_site_id.as_ref()) {', 2),
        ('let placed_at = now();', 'let eager_site_id = site_id();\n    let placed_at = now();', 1),
    ], 'CACHE native host call sequence'),
    ('store read after clock', [
        ('store_orders()\n                .iter()', 'captured_store_orders\n                .iter()', 1),
        ('let now = now();', 'let captured_store_orders = store_orders();\n    let now = now();', 1),
    ], 'CACHE native number price'),
    ('read-only store reentry', [
        ('        preserved.push(', '        let _held_cache_borrow = cache.entries.borrow_mut();\n        preserved.push(', 1),
    ], 'CACHE native read-only store reentry'),
]
results = []
def run():
    return subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
baseline = run()
assert baseline.returncode == 0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout, baseline.stdout
try:
    for name, changes, assertion in controls:
        text = original.decode()
        for before, after, expected_occurrences in changes:
            assert text.count(before) == expected_occurrences, (name, text.count(before))
            text = text.replace(before, after, 1)
        source.write_text(text)
        broken = run()
        detected = broken.returncode == 101 and 'panicked at' in broken.stdout and assertion in broken.stdout
        source.write_bytes(original)
        restored = run()
        passed = restored.returncode == 0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
        result = dict(family=name, assertion=assertion, mutation_exit=broken.returncode, named_runtime_failure=detected, restored_exit=restored.returncode, restored_pass=passed)
        results.append(result)
        print(json.dumps(result), flush=True)
        if not detected or not passed:
            print(broken.stdout, restored.stdout, flush=True)
            raise AssertionError('surviving or invalid control: ' + name)
finally:
    source.write_bytes(original)
    evidence.parent.mkdir(parents=True, exist_ok=True)
    evidence.write_text(json.dumps(dict(gate_sha=gate_sha, baseline_exit=baseline.returncode, command=command, controls=results, enumeration_floor=8), indent=2) + '\n')
assert source.read_bytes() == original
print('Eight controls detected; exact production restoration verified.', flush=True)
