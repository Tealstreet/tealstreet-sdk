"""Compile one production mutation per market-read family, then restore and retest."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/market_reads.rs'
out = root / 'sdk-rs/target/weex-market-reads-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'market_reads']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('market-margin-filter', 'fetch_markets', '(rt.get)(host, row, "marginAsset")?', '(rt.get)(host, row, "quoteAsset")?', 'native_result::markets_filter'),
    ('ticker-book-fields', 'fetch_tickers', '(rt.normalize_ticker)(host, ticker, market, book)?', '(rt.normalize_ticker)(host, book, market, ticker)?', 'native_result::tickers_join'),
    ('historical-candle-route', 'fetch_ohlcv', '"HISTORY_KLINES"', '"KLINES"', 'native_requests::candles_historical_zero'),
    ('candle-listener-method', 'listen_ohlcv', '"subscribeCandle"', '"subscribeOrderbook"', 'native_result::listen_candle_provider'),
    ('depth-update-id', 'fetch_order_book', '(rt.optional_get)(host, &data, "lastUpdateId")?', '(rt.optional_get)(host, &data, "lastSequence")?', 'native_result::depth_default'),
    ('public-trade-lower-limit', 'fetch_public_trades', 'math_max((rt.to_number)(host, &limit)?, 1.0)', 'math_max((rt.to_number)(host, &limit)?, 2.0)', 'native_requests::public_limit_lower'),
    ('execution-from-filter', 'fetch_user_trade_rows', '(rt.less_than)', '(rt.greater_than)', 'native_result::query_filters'),
    ('execution-sort-direction', 'fetch_historical_fills', 'sort_times(host, rt, filtered, true)', 'sort_times(host, rt, filtered, false)', 'native_result::history_order'),
]
results = []
for name, function, before, after, assertion in controls:
    original = source.read_bytes()
    text = original.decode()
    start = text.index('pub fn ' + function + '<')
    end = text.find('\npub fn ', start + 1)
    if end < 0:
        end = len(text)
    segment = text[start:end]
    assert segment.count(before) == 1, (name, segment.count(before))
    try:
        source.write_text(text[:start] + segment.replace(before, after) + text[end:])
        broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-broken.log')).write_text(broken.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(restored.stdout)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    result = {'control': name, 'function': function, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode, 'namedFailure': assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout}
    results.append(result)
    (out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert broken.returncode == 101 and result['namedFailure'], (name, result)
(out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
