#!/usr/bin/env python3
"""Sequential private mutations; always restore exact source bytes, never fixtures."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom'
RUST = CRATE / 'src/types.rs'
TS = ROOT / 'packages/safe-cex/src/exchanges/cryptocom/cryptocom.types.ts'
CARGO = '/opt/homebrew/opt/rustup/bin/cargo'
ENV = os.environ.copy()
assert ENV.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
COMMAND = [CARGO, 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-cryptocom', '--test', 'types', '--jobs', '1', '--locked']


def run(env=ENV):
    return subprocess.run(COMMAND, cwd=ROOT, env=env, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


def passed(result):
    assert result.returncode == 0, result.stdout
    assert '3 passed; 0 failed' in result.stdout, result.stdout


def control(name, changes, assertion, env=ENV):
    originals = {file: file.read_bytes() for file, _, _ in changes}
    try:
        for file, old, new in changes:
            text = file.read_text()
            assert text.count(old) == 1, (name, old, text.count(old))
            file.write_text(text.replace(old, new, 1))
        result = run(env)
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        for file, original in originals.items():
            file.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)


passed(run())
controls = [
    ('base_url', 'https://api.crypto.com/exchange/v1', 'https://wrong.invalid', 'constant_value:BASE_URLS'),
    ('ws_user_market_swap', 'wss://stream.crypto.com/exchange/v1/user', 'wss://stream.crypto.com/exchange/v1/market', 'constant_value:WSS_URLS'),
    ('ping_threshold', 'f64 = 30_000.0', 'f64 = 31_000.0', 'constant_bits:CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS'),
    ('reconnect_threshold', 'f64 = 90_000.0', 'f64 = 91_000.0', 'constant_bits:CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS'),
    ('endpoint_value', '/public/get-insurance', '/public/wrong-insurance', 'constant_value:ENDPOINTS'),
    ('public_endpoint_membership', 'ENDPOINTS[8].1', 'ENDPOINTS[9].1', 'constant_value:PUBLIC_ENDPOINTS'),
    ('grafana_every', 'ENDPOINTS[10].1', 'ENDPOINTS[11].1', 'constant_value:GRAFANA_ENDPOINTS_EVERY'),
    ('grafana_periodic', 'ENDPOINTS[9].1', 'ENDPOINTS[10].1', 'constant_value:GRAFANA_ENDPOINTS_PERIODIC'),
    ('interval_case', '("1d", "1D")', '("1d", "1d")', 'interval_value:1d'),
    ('interval_error', 'Unsupported Crypto.com candlestick interval:', 'Wrong interval:', 'interval_error:3m'),
    ('inherited_type', 'TableHit::Inherited(member) => TableHit::Inherited(member)',
     'TableHit::Inherited(member) => TableHit::Mapped(member.js_string())', 'interval_type:toString'),
]
for name, old, new, assertion in controls:
    control(name, [(RUST, old, new)], assertion)
control('signed_zero_before_json', [
    (RUST, 'f64 = 30_000.0', 'f64 = 0.0'),
    (TS, 'CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS = 30_000', 'CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS = -0'),
], 'constant_bits:CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS')
missing = dict(ENV)
missing.pop('SAFE_CEX_TYPESCRIPT_PATH')
control('missing_compiler', [], 'SAFE_CEX_TYPESCRIPT_PATH is required', missing)
print('13 controls failed by name and passed after restoration', flush=True)
