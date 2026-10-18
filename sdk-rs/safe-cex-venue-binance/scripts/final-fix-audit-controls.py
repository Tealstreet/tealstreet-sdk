#!/usr/bin/env python3
"""Compile independent faults in the live readers, then restore exact bytes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

SDK = Path(__file__).resolve().parents[2]
CRATE = SDK / 'safe-cex-venue-binance'
LOGS = SDK / 'target/binance-final-fix-audit-controls'
LOGS.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'CARGO_INCREMENTAL': '0'}


def run(label, selected):
    result = subprocess.run([
        'cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-binance',
        '--all-features', '--test', 'private_classes', selected,
        '--', '--include-ignored',
    ], cwd=SDK, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (LOGS / f'{label}.log').write_text(result.stdout)
    return result


controls = [
    ('market-admission', 'exchange_classes.rs', None,
     'if !invalid.is_empty() {', 'if false && !invalid.is_empty() {', 'market_',
     ['production_native_futures_market_asset_refusals_keep_siblings',
      'production_native_pm_market_asset_refusals_keep_siblings',
      'production_native_spot_market_asset_refusals_keep_siblings']),
    ('futures-position', 'position_snapshots.rs', 'pub async fn fetch_futures_positions(',
     'number(p.read("entryPrice")?.float()?)', 'number(p.read("entryPrice")?.float()? + 1.0)',
     'native_final_audit_position', ['native_final_audit_position_readers_keep_fields_without_generated_dtos']),
    ('pm-position', 'position_snapshots.rs', 'pub async fn fetch_pm_positions(',
     'number(p.read("entryPrice")?.float()?)', 'number(p.read("entryPrice")?.float()? + 1.0)',
     'native_final_audit_position', ['native_final_audit_position_readers_keep_fields_without_generated_dtos']),
    ('income-result', 'rest_history.rs', None,
     'out.write("income", number(r.read("income")?.float()?));',
     'out.write("income", number(r.read("income")?.float()? + 1.0));',
     'native_income_range_retains_source_number_profiles',
     ['native_income_range_retains_source_number_profiles']),
    ('income-options', 'rest_history.rs', 'async fn income(',
     'fallback(optional(&opts, "limit")?, number(1000.0)).number()?', 'number(3.0).number()?',
     'native_factory_retries_unsigned_listen_post_and_encodes_signed_query',
     ['native_factory_retries_unsigned_listen_post_and_encodes_signed_query']),
]
receipts = []
for label, source, anchor, before, after, selected, names in controls:
    path = CRATE / 'src' / source
    original = path.read_bytes()
    text = original.decode()
    start = text.index(anchor) if anchor else 0
    offset = text.index(before, start)
    changed = text[:offset] + text[offset:].replace(before, after, 1)
    baseline = run(label + '-baseline', selected)
    assert baseline.returncode == 0, baseline.stdout
    try:
        path.write_text(changed)
        red = run(label + '-red', selected)
        assert red.returncode == 101, red.stdout
        assert 'error[E' not in red.stdout and 'test result: FAILED.' in red.stdout, red.stdout
        for name in names:
            assert any(name + ' ... FAILED' in line for line in red.stdout.splitlines()), red.stdout
        assert 'panicked at' in red.stdout, red.stdout
        print(label, 'compiled RED', flush=True)
    finally:
        path.write_bytes(original)
        assert path.read_bytes() == original
        green = run(label + '-restored', selected)
        assert green.returncode == 0, green.stdout
    for name in names:
        assert any(name + ' ... ok' in line for line in green.stdout.splitlines()), green.stdout
    receipts.append({'control': label, 'source': source, 'before': before, 'after': after,
                     'restored_sha256': hashlib.sha256(original).hexdigest(),
                     'named_tests': names, 'compiled_red': red.returncode, 'restored_green': green.returncode})
    print(label, 'exact restore GREEN', flush=True)
(LOGS / 'receipt.json').write_text(json.dumps(receipts, indent=2) + '\n')
print('ALL FIVE LIVE-BINDING CONTROLS COMPLETE', flush=True)
