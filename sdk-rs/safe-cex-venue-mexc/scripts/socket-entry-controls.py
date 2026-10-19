#!/usr/bin/env python3
"""Compile independent socket startup/iteration faults and restore exact source."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['CARGO_INCREMENTAL'] = '0'
controls = [
    ('book-prefix', 'ws_public.rs',
     'let snapshot = safe_cex_exchange::eager::start_eager(&parent.base().runtime, async move {\n            Ok(request_parent\n                .fetch_order_book(request_symbol, Some(100.))\n                .await)\n        });',
     'let snapshot = async move { Ok::<_, ExchangeError>(request_parent.fetch_order_book(request_symbol, Some(100.)).await) };',
     'native_book_entry::registered_book_entry_starts_before_native_listener_returns',
     'MEXC BOOK ENTRY starts the actual snapshot before returning its listener'),
    ('balance-prefix', 'sockets.rs',
     'let balance =\n                safe_cex_exchange::eager::start_eager(&parent.base().runtime, async move {\n                    Ok(request_owner.fetch_balance().await)\n                });',
     'let balance = async move { Ok::<_, ExchangeError>(request_owner.fetch_balance().await) };',
     'native_socket_audit::private_balance_timer_starts_http_before_its_callback_returns',
     'MEXC BALANCE ENTRY starts HTTP before returning from the actual NativeRuntime timer callback'),
    ('live-resubscribe', 'ws_public.rs',
     'cursor = entry.sequence;',
     'cursor = u64::MAX;',
     'registered_public_resubscribe_visits_send_time_insertions',
     'MEXC constructed resubscribe traverses live registry after send-time insertion'),
]
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-mexc', '--test', 'socket_assembly']
for name, file, before, after, test, assertion in controls:
    path = crate / 'src' / file
    original = path.read_bytes()
    source = original.decode()
    if source.count(before) != 1:
        raise SystemExit(f'{name}: expected exactly one mutation anchor')
    try:
        path.write_text(source.replace(before, after))
        result = subprocess.run(command + ['--', test, '--exact', '--nocapture'], cwd=sdk, env=env, capture_output=True, text=True)
        log = result.stdout + result.stderr
        Path(f'/tmp/mexc-non-utf8-control-{name}.log').write_text(log)
        if result.returncode != 101 or f'{test} ... FAILED' not in log or assertion not in log:
            raise SystemExit(f'{name}: expected compiled named assertion RED, got {result.returncode}\n{log}')
    finally:
        path.write_bytes(original)
    result = subprocess.run(command + ['--', test, '--exact', '--nocapture'], cwd=sdk, env=env, capture_output=True, text=True)
    log = result.stdout + result.stderr
    Path(f'/tmp/mexc-non-utf8-control-{name}-restored.log').write_text(log)
    if result.returncode != 0 or '1 passed; 0 failed' not in log:
        raise SystemExit(f'{name}: exact restoration failed\n{log}')
    print(f'{name}: compiled named RED -> exact-restored GREEN', flush=True)
