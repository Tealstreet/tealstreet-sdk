#!/usr/bin/env python3
"""Compiled faults in the production polling continuation consumers."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-polling-alias-controls-'))
path = sdk / 'safe-cex-venue-kraken/src/ohlcv_polling.rs'
cases = [
    ('PollFuture', 'poll_future_retains_held_native_http_completion_and_latest_candle',
     'host.spawn(poll());', 'host.spawn(Box::pin(async { Ok(()) }));'),
    ('PollCallback', 'poll_callback_runs_again_at_source_five_second_delay_without_duplicate_emit',
     'h.set_timeout(poll, 5000.0)?', 'h.set_timeout(poll, 5001.0)?'),
    ('CandleCallback', 'candle_callback_keeps_latest_native_cell_identity_and_changed_fields',
     '(self.callback)(latest.clone())?;', '(self.callback)(previous.clone())?;'),
    ('Cleanup', 'cleanup_alias_removes_native_cache_timer_and_stops_future_http',
     'host.map_delete(Slot::Candle, &key)', 'Ok(())'),
]
results = []
for alias, test, needle, replacement in cases:
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_polling_aliases::{test}'
    def run(label, negative):
        result = subprocess.run([
            'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken',
            '--lib', name, '--', '--exact', '--nocapture',
        ], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{alias}-{label}.log').write_text(result.stdout)
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
    results.append({'alias': alias, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'{alias}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
