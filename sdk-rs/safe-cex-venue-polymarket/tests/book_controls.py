"""Order-book controls, serial and restored; one actual TS capture per run."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
rust = crate / 'src/types.rs'
source = crate.parents[1] / 'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'auth_types', '--locked', '--jobs', '1', '--', 'order_book_record_actual_typescript', '--nocapture']

def run(name):
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.log')).write_text(f'exit_code={result.returncode}\n' + result.stdout)
    assert result.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, result.stdout
    return result

def change_book(text, before, after):
    start = text.index('pub struct PolymarketOrderBook {')
    return text[:start] + text[start:].replace(before, after)

def swap(text):
    start = text.index('pub struct PolymarketOrderBook {')
    region = text[start:]
    for field, wire in [('bids', 'asks'), ('asks', 'bids')]:
        region = region.replace('    pub ' + field + ': Vec<PolymarketOrderBookLevel>,', '    #[serde(rename = "' + wire + '")]\n    pub ' + field + ': Vec<PolymarketOrderBookLevel>,')
        region = region.replace('            ' + field + ': Vec<PolymarketOrderBookLevel>,', '            #[serde(rename = "' + wire + '")]\n            ' + field + ': Vec<PolymarketOrderBookLevel>,')
    return text[:start] + region

controls = [
    ('book-bids-asks-swap', rust, swap, 'native_book.bids[0].price'),
    ('book-bids-reversed', rust, lambda s: change_book(s, 'bids: fields.bids,', 'bids: fields.bids.into_iter().rev().collect(),'), 'native_book.bids[0].price'),
    ('book-asks-truncated', rust, lambda s: change_book(s, 'asks: fields.asks,', 'asks: fields.asks.into_iter().take(1).collect(),'), 'native_book.asks.length'),
    ('book-hash-null-collapse', rust, lambda s: change_book(s, '#[serde(default, deserialize_with = "present")]', '#[serde(default)]'), 'present_null:PolymarketOrderBook.hash'),
    ('book-time-null-collapse', rust, lambda s: change_book(s, '#[serde(default, deserialize_with = "present_number")]', '#[serde(default)]'), 'present_null:PolymarketOrderBook.timestamp'),
    ('book-sign-loss', rust, lambda s: change_book(s, 'timestamp: fields.timestamp,', 'timestamp: fields.timestamp.map(|v| v + 0.0),'), 'numeric ingress changed Number bits at $.timestamp'),
    ('book-actual-array-element', source, lambda s: s.replace('bids: PolymarketOrderBookLevel[];', 'bids: PolymarketPrice[];'), 'declared field token_id'),
]
assert run('book-baseline').returncode == 0
for name, file, change, assertion in controls:
    original = file.read_text()
    changed = change(original)
    assert changed != original, name + ': mutation did not apply'
    try:
        file.write_text(changed)
        result = run(name + '-mutation')
        assert result.returncode == 101 and assertion in result.stdout, result.stdout
        print(name + ': FAIL ' + assertion, flush=True)
    finally:
        file.write_text(original)
    assert run(name + '-restored').returncode == 0
    print(name + ': RESTORED PASS', flush=True)
