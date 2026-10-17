"""Compile production mutations and require native assertion failures, then restore."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/public_feed.rs'
out = root / 'sdk-rs/target/weex-public-feed-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '1', '--test', 'public_feed']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
text = source.read_text()
kline_start = text.index('let route = (rt.route_method)(host)?;', text.index('"kline")'))
kline_end = text.index('        (rt.route)(host, route, (rt.string_units)(units), &json)', kline_start)
kline = text[kline_start:kline_end]
late_capture = kline.replace('let route = (rt.route_method)(host)?;\n        ', '', 1) + '        let route = (rt.route_method)(host)?;\n'
controls = [
    ('constructor-pong-default', 'construct_public_websocket', '(rt.boolean)(true)', '(rt.boolean)(false)', 'public_feed::constructor_native constructor_defaults'),
    ('public-transport-header', 'connect_and_subscribe', '(rt.text)("Tealstreet")', '(rt.text)("WEEX")', 'public_feed::connect_native connect_basic'),
    ('public-pong-interval', 'on_message', '15_000.0', '15_001.0', 'public_feed::message_native message_ping'),
    ('candle-price-topic', 'listen_ohlcv', '"_LAST_PRICE"', '"_MARK_PRICE"', 'public_feed::candle_native candles_supported'),
    ('snapshot-subscribe-before-callback', 'run_public_callback', '(rt.invoke)(host, callback, value)?;\n            (rt.subscribe)(host, topic)', '(rt.subscribe)(host, topic)?;\n            (rt.invoke)(host, callback, value)', 'public_feed::book_native book_callback_rejection'),
    ('public-dispose-off', 'dispose', '"off"', '"on"', 'public_feed::dispose_native dispose_off_failure'),
    ('ticker-mark-index-swap', 'handle_ticker', '("mark", mark),\n        ("index", index),', '("mark", index),\n        ("index", mark),', 'public_feed::message_native message_ticker'),
    ('depth-price-amount-swap', 'handle_depth', '(rt.update_price)(host, &book, side, price, amount)', '(rt.update_price)(host, &book, side, amount, price)', 'public_feed::message_native message_depth'),
    ('dynamic-ticker-dispatch', 'on_message', '(rt.invoke)(host, &(rt.this_property)(host, "handleTicker")?, json)', 'handle_ticker(host, rt, json)', 'public_feed::message_native message_dynamic_ticker'),
    ('routed-method-capture', 'on_message', kline, late_capture, 'public_feed::message_native message_coercion_order'),
    ('gap-silence', 'handle_depth', 'WeexPublicCallback::GapSnapshotFailure,', 'WeexPublicCallback::SnapshotFailure { symbol },', 'public_feed::depth_native depth_gap_reject'),
]
selected = os.environ.get('WEEX_PUBLIC_FEED_CONTROL')
controls = [control for control in controls if selected is None or control[0] == selected]
assert controls, ('unknown control', selected)
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
    result = {'control': name, 'function': function, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode, 'namedFailure': assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout}
    results.append(result)
    (out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    assert broken.returncode == 101 and result['namedFailure'], (name, result)
