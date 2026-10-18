#!/usr/bin/env python3
"""Named compiled faults in production socket/timer callback consumers."""
from pathlib import Path
import json
import os
import subprocess
import tempfile

os.environ['CARGO_INCREMENTAL'] = '0'
sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-loop-socket-alias-controls-'))
cases = [
    ('socket-event', 'sockets.rs', 'socket_callback_reaches_real_message_dispatch_and_native_store',
     'let result = this.call_method(member, vec![event]).map_err(ex)?;',
     'let result = { let _ = event; this.call_method(member, vec![Js::Undefined]).map_err(ex)? };'),
    ('timer-callback', 'host_typed.rs', 'tick_callback_starts_real_signed_sync_only_at_its_deadline',
     'let future = callback().map_err(Thrown::from)?;',
     "let future = { let _ = &callback; Box::pin(async { Ok(()) }) as ExchangeFuture<'static, ()> };"),
    ('timer-refusal', 'host_typed.rs', 'tick_callback_reports_synchronous_and_detached_future_refusals',
     'reporter.report(&Thrown::from(error), "kraken tick");',
     'let _ = (reporter, error);'),
]
results = []
for label, filename, test, needle, replacement in cases:
    path = sdk / 'safe-cex-venue-kraken/src/binding' / filename
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_loop_socket_aliases::{test}'
    def run(stage, negative):
        result = subprocess.run([
            'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken',
            '--lib', name, '--', '--exact', '--nocapture',
        ], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{label}-{stage}.log').write_text(result.stdout)
        if negative:
            assert result.returncode != 0 and f'test {name} ... FAILED' in result.stdout and 'panicked at' in result.stdout and 'could not compile' not in result.stdout, result.stdout
        else:
            assert result.returncode == 0 and f'test {name} ... ok' in result.stdout, result.stdout
        return result.returncode
    try:
        path.write_text(source.replace(needle, replacement))
        red = run('red', True)
    finally:
        path.write_bytes(original)
    green = run('restored', False)
    results.append({'control': label, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'{label}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
