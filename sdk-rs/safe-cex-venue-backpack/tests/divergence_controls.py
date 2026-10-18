#!/usr/bin/env python3
"""Compiled production faults, named assertion RED, exact-byte restore, GREEN."""
import os
from pathlib import Path
import re
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
logs = workspace / 'target/backpack-divergence-controls'
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL="0")
common_git = Path(subprocess.check_output(
    ['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'],
    cwd=workspace, text=True).strip())
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(common_git.parent / 'node_modules/typescript'))

def body(source, name):
    match = re.search(r'\bfn ' + re.escape(name) + r'\(', source)
    assert match, name
    start = source.index('{', match.end()) + 1
    depth, end = 1, start
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return start, end - 1

def replace_once(source, before, after):
    assert source.count(before) == 1, before
    return source.replace(before, after)

def run(name, phase, test):
    package = 'safe-cex-exchange' if name == 'core-drop-incomplete' else 'safe-cex-venue-backpack'
    target = 'native_final_drop' if name == 'core-drop-incomplete' else 'divergence_binding'
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', package,
               '--all-features', '--test', target, test, '--', '--exact']
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / f'{name}-{phase}.log').write_text(output)
    return result.returncode, output

lazy = lambda s: replace_once(s, 'safe_cex_exchange::eager::start_eager(&self.base.runtime, future)', 'future')
visible = lambda s: replace_once(s, 'let _ = future.await;', 'future.await?;')
def lazy_private(s):
    s = replace_once(s, 'safe_cex_exchange::eager::start_eager(&runtime, async move {', 'Box::pin(async move {\n            let _ = runtime;')
    return s
controls = [
 ('account-lazy', 'adapter.rs', 'get_account', lazy, 'real_account_catch_survives_dropped_sdk_waiter'),
 ('balance-lazy', 'adapter.rs', 'fetch_balance', lazy, 'real_balance_catch_survives_dropped_waiter'),
 ('orders-visible', 'private_adapter.rs', 'fetch_orders_throttled', visible, 'real_orders_reconciliation_silently_catches_failure'),
 ('balance-visible', 'private_adapter.rs', 'execute_balance_fetch', visible, 'real_balance_reconciliation_silently_catches_failure'),
 ('orders-canceled', 'private_adapter.rs', 'fetch_orders_throttled', lazy_private, 'real_reconciliation_continues_after_dropped_waiters'),
 ('balance-canceled', 'private_adapter.rs', 'execute_balance_fetch', lazy_private, 'real_reconciliation_continues_after_dropped_waiters'),
 ('market-owner-retained', 'adapter_runtime.rs', 'subscribe_to_all_markets', lambda _: '\n        source::subscribe_to_all_markets(Data::Markets(markets), self.frame())\n    ', 'real_pending_market_timer_does_not_retain_final_exchange'),
 ('loop-owner-retained', 'adapter_runtime.rs', 'schedule_loop', lambda _: '\n        self.timeout(cb, d)\n    ', 'real_pending_poll_loop_does_not_retain_final_exchange'),
 ('drop-incomplete', 'adapter.rs', 'drop', lambda _: '\n    ', 'real_final_drop_fully_tears_down_public_socket'),
 ('core-drop-incomplete', '../../safe-cex-exchange/src/native_teardown.rs', 'native_final_drop', lambda _: '\n    let _ = (base, venue_cleanup);\n', 'native_final_drop_completes_after_both_venue_and_store_errors'),
 ('held-throttle', 'public_adapter.rs', 'call', lambda _: '\n        let _ = (data, self.window_ms());\n        Ok(())\n    ', 'real_public_receiver_stays_live_while_exchange_is_held'),
]
selected = set(sys.argv[1:])
assert selected.issubset({row[0] for row in controls}), 'unknown control name'
for name, file, function, fault, test in controls:
    if selected and name not in selected:
        continue
    path = crate / 'src' / file
    exact = path.read_bytes()
    source = exact.decode()
    start, end = body(source, function)
    code, _ = run(name, 'baseline', test)
    assert code == 0, f'{name}: baseline failed'
    try:
        path.write_text(source[:start] + fault(source[start:end]) + source[end:])
        code, output = run(name, 'red', test)
        assert code != 0 and f'test {test} ... FAILED' in output and 'error: could not compile' not in output, f'{name}: not a compiled named RED; inspect {logs}'
    finally:
        path.write_bytes(exact)
        assert path.read_bytes() == exact
    code, _ = run(name, 'restored', test)
    assert code == 0, f'{name}: exact restore failed'
    print(f'{name}: compiled RED -> exact restore GREEN', flush=True)
