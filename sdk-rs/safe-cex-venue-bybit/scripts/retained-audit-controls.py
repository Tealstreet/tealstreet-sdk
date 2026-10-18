#!/usr/bin/env python3
"""Compile native retention faults, then restore production bytes exactly."""
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
base = sdk / 'safe-cex-venue-bybit/src/binding/members'
controls = [
    ('ticker-cache', 'markets.rs', 'return store.get("tickers");',
     'return Ok(Js::array(vec![]));', 'retained_audit_ticker_native_lookup_and_prefix', 'ticker_cached_identity'),
    ('ticker-prefix', 'markets.rs', 'let last = row.get("lastPrice")?.parse_float()?;',
     'let last = row.get("lastPrice")?.to_number()?;', 'retained_audit_ticker_native_lookup_and_prefix', 'ticker_utf16_prefix'),
    ('spot-info', 'spot.rs', '&to_retained_input(&row)?,',
     '&to_retained_input(&crate::binding::value::from_json(&crate::binding::value::to_json(&row)?))?,',
     'retained_audit_spot_market_native_info_and_eager_request', None),
    ('spot-warning', 'spot.rs', 'vec![Js::utf16(&message), Js::str("warning")]',
     'vec![Js::str(&String::from_utf16_lossy(&message)), Js::str("warning")]',
     'retained_audit_startup_dropped_waiter_and_native_warning', 'startup_native_warning'),
]
env = dict(os.environ, CARGO_INCREMENTAL='0')
assert not env.get('RUSTC_BOOTSTRAP'), 'use the workflow Cargo profile'
logs = Path(tempfile.mkdtemp(prefix='bybit-retained-audit-controls-'))
print(logs, flush=True)
for name, filename, before, after, test, label in controls:
    path = base / filename
    original = path.read_bytes()
    assert original.decode().count(before) == 1, (name, 'source anchor changed')
    command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-bybit', '--lib', test, '--jobs', '2', '--', '--test-threads=1']
    try:
        path.write_text(original.decode().replace(before, after))
        result = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
        (logs / (name + '.log')).write_text(result.stdout)
        assert result.returncode and 'Finished `test`' in result.stdout and test + ' ... FAILED' in result.stdout, (name, logs)
        if label:
            assert label in result.stdout, (name, logs)
        print(name + ': compiled named RED', flush=True)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    result = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    (logs / (name + '-restored.log')).write_text(result.stdout)
    assert not result.returncode and '1 passed; 0 failed' in result.stdout, (name, logs)
    print(name + ': exact restore GREEN', flush=True)
