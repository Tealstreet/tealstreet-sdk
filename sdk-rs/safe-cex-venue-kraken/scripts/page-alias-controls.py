#!/usr/bin/env python3
"""Named compiled faults in the real Kraken page producer and pager callback."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-page-alias-controls-'))
cases = [
    ('PageFuture', 'binding/host_more.rs',
     'page_future_keeps_held_native_http_completion_and_exact_wire_params',
     'Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })',
     'Ok(async move { let _ = await_js(&self.exchange_runtime(), promise).await?; Ok(h(Js::array(vec![]))) })'),
    ('PageCallback', 'private_accounts.rs',
     'income_range_uses_page_callback_with_source_window_and_page_limit',
     '("pageLimit", h.number(500.0)),', '("pageLimit", h.number(37.0)),'),
]
results = []
for alias, file, test, needle, replacement in cases:
    path = sdk / 'safe-cex-venue-kraken/src' / file
    original = path.read_bytes()
    name = f'binding::exchange::accepted_page_aliases::{test}'
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
    source = original.decode()
    # The page producer shares a spelling with income_range; change only the
    # account_log_page method, and require that method's exact single match.
    if alias == 'PageFuture':
        start = source.index('    fn account_log_page(')
        end = source.index('    fn income_range(', start)
        body = source[start:end]
        assert body.count(needle) == 1
        mutated = source[:start] + body.replace(needle, replacement) + source[end:]
    else:
        assert source.count(needle) == 1
        mutated = source.replace(needle, replacement)
    try:
        path.write_text(mutated)
        red = run('red', True)
    finally:
        path.write_bytes(original)
    green = run('restored', False)
    results.append({'alias': alias, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'{alias}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
