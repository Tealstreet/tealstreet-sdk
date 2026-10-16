#!/usr/bin/env python3
"""Compiled method defects, with one shared delay control for both debounce rows."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('message_dispatch.rs', 'futures_public', '("ticker", "handleTicker")', '("ticker", "handleOHLC")', 'message_dispatch_live_source', 'DISPATCH native UTF16'),
    ('message_dispatch.rs', 'spot_message', '("ticker", "handleTicker")', '("ticker", "handleOHLC")', 'message_dispatch_live_source', 'DISPATCH native UTF16'),
    ('message_dispatch.rs', 'spot_message', '("executions", "handleExecutions")', '("executions", "handleBalances")', 'message_dispatch_live_source', 'DISPATCH native UTF16'),
    ('message_dispatch.rs', 'futures_private', '("fills", "handleFills")', '("fills", "handleBalances")', 'message_dispatch_live_source', 'DISPATCH native UTF16'),
    ('private_control.rs', 'perform_auth', 'kind: "ws-auth"', 'kind: "rest-auth"', 'perform_auth_live_source', 'AUTH native UTF16'),
    ('private_control.rs', 'subscribe_private_feeds', '"account_balances"', '"balances"', 'subscriptions_live_source', 'SUBSCRIBE native UTF16'),
    ('private_control.rs', 'throttle', 'callback, 1000', 'callback, 1001', ('futures_throttle_live_source', 'spot_throttle_live_source'), 'REFRESH native number'),
    ('private_control.rs', 'futures_fetch_throttled_orders', 'orders, loaded, true', 'orders, loaded, false', 'futures_refresh_live_source', 'REFRESH native bool'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name in {c[0] for c in controls}}
for name, function, old, new, test, assertion in controls:
    production = crate / 'src' / name
    original = originals[name]
    source = original.decode()
    start = source.index('fn ' + function + '<')
    end = source.find('\n}', start) + 2
    body = source[start:end]
    assert body.count(old) == 1, (function, 'unique mutation anchor', old)
    tests = (test,) if isinstance(test, str) else test
    def run(selected):
        command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--test', 'message_control', selected, '--jobs', '2']
        return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    for selected in tests:
        baseline = run(selected)
        assert baseline.returncode == 0, baseline.stdout
    try:
        production.write_text(source[:start] + body.replace(old, new) + source[end:])
        for selected in tests:
            broken = run(selected)
            assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (function, old, selected, broken.stdout)
            print(f'{function} {selected} {old}: compiled exit 101; {assertion}', flush=True)
    finally:
        if production.read_bytes() != original:
            production.write_bytes(original)
    for selected in tests:
        restored = run(selected)
        assert restored.returncode == 0, restored.stdout
        assert production.read_bytes() == original
        print(f'{function} {selected}: restored exit 0', flush=True)
print(f'{revision}: 9 rows, 8 production defects, 9 named compiled failures, 9 restored passes; debounce rows selected separately')
