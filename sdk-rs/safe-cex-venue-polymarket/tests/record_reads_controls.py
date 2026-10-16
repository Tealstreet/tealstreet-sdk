"""Serial runtime faults for the four public record reads; always restore files."""
from pathlib import Path
import json, os, subprocess
crate = Path(__file__).resolve().parents[1]
rust = crate / 'src/public_reads.rs'
types = crate / 'src/types.rs'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
def replace(old, new):
    def change(source):
        assert source.count(old) == 1, old
        return source.replace(old, new)
    return change
def market_swap(source):
    start = source.index("impl<'de> Deserialize<'de> for PolymarketClobMarket")
    prefix, tail = source[:start], source[start:]
    return prefix + replace('minimum_order_size: fields.minimum_order_size,\n            minimum_tick_size: fields.minimum_tick_size,', 'minimum_order_size: fields.minimum_tick_size,\n            minimum_tick_size: fields.minimum_order_size,')(tail)
def book_swap(source):
    return replace('bids: fields.bids,\n            asks: fields.asks,', 'bids: fields.asks,\n            asks: fields.bids,')(source)
def price_swap(source):
    start = source.index("impl<'de> Deserialize<'de> for PolymarketPrice")
    end = source.index('impl JsNumberOutput for PolymarketPrice', start)
    return source[:start] + replace('token_id: fields.token_id,\n            price: fields.price,', 'token_id: fields.price,\n            price: fields.token_id,')(source[start:end]) + source[end:]
# Consume the successful batch into an empty Vec; leave the single-book method intact.
def empty_books(source):
    start = source.index('pub async fn get_order_books(')
    end = source.index('/// Source `PolymarketAPI.getPrices`', start)
    return source[:start] + replace('    .await\n}', '    .await.map(|mut rows: Vec<PolymarketOrderBook>| { rows.clear(); rows })\n}')(source[start:end]) + source[end:]
controls = [
    ('markets-size-tick-swap', 'record_native_minimum_order_size', [(types, market_swap)]),
    ('book-bid-ask-swap', 'record_native_bid_count', [(types, book_swap)]),
    ('books-response-empty', 'record_native_book_count', [(rust, empty_books)]),
    ('prices-token-price-swap', 'record_native_price_token_id', [(types, price_swap)]),
]
assert len(controls) == 4, 'record_control_floor'
assert controls[-1][0] == 'prices-token-price-swap', 'record_control_last_iteration'

command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'record_reads', '--locked', '--jobs', '2', '--', '--nocapture']
def run(name):
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.log')).write_text(f'exit_code={result.returncode}\n' + result.stdout)
    assert result.stdout.count('polymarket record reads oracle: one Node invocation for the entire corpus') == 1, result.stdout
    return result
assert not subprocess.check_output(['git', 'status', '--porcelain', '--', str(crate), str(crate.parent/'parity/manifest/exchanges/polymarket')], cwd=crate, text=True).strip(), 'commit lane files before binding gates to HEAD'
baseline = run('baseline')
assert baseline.returncode == 0, baseline.stdout
summary = {'sha': subprocess.check_output(['git','rev-parse','HEAD'], cwd=crate, text=True).strip(), 'baseline_exit': baseline.returncode, 'floor': '4 controls, stopped after prices-token-price-swap', 'controls': []}
for name, assertion, changes in controls:
    originals = {p: p.read_text() for p, _ in changes}
    try:
        for p, change in changes:
            p.write_text(change(p.read_text()))
        mutated = run(name + '-mutation')
        caught = mutated.returncode == 101 and assertion in mutated.stdout
    finally:
        for p, text in originals.items():
            p.write_text(text)
    restored = run(name + '-restored')
    summary['controls'].append({'name': name, 'mutation_exit': mutated.returncode, 'assertion': assertion, 'caught': caught, 'restored_exit': restored.returncode})
    (logs / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    assert restored.returncode == 0, restored.stdout
    assert caught, f'FINDING survivor or wrong failure at {name}:\n{mutated.stdout}'
    print(f'{name}: 101 {assertion}; restored 0', flush=True)
