"""Compile one production control per new trading family and restore each source."""
from pathlib import Path
import json
import os
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/trading.rs'
out = root / 'sdk-rs/target/weex-trading-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'trading']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('order-amount-filled', 'normalize_order', '("amount", amount),\n            ("filled", filled),', '("amount", filled),\n            ("filled", amount),', 'trading::normalizeOrder_native order_distinct'),
    ('algo-virtual-id', 'normalize_algo_order', '"__tp"', '"__sl"', 'trading::normalizeAlgoOrder_native algo_virtual_pair'),
    ('position-pnl-fields', 'normalize_position', '("unrealizedPnl", pnl),\n        ("realizedPnl", realized),', '("unrealizedPnl", realized),\n        ("realizedPnl", pnl),', 'trading::normalizePosition_native position_distinct'),
    ('payload-quantity-price', 'format_order_payload', '("quantity", qty),\n            ("price", price),', '("quantity", price),\n            ("price", qty),', 'trading::formatOrderPayload_native payload_limit'),
    ('single-order-endpoint', 'place_order', '"ORDER"', '"ALGO_ORDER"', 'trading::placeOrder_native single_regular'),
    ('multi-order-batch-cap', 'place_orders', '== 5', '== 4', 'trading::placeOrders_native orders_chunking'),
    ('batch-order-id', 'place_regular_order_batch', 'Box::new(|result, _| (r.get)(h, &result, "orderId"))', 'Box::new(|result, _| (r.get)(h, &result, "clientOrderId"))', 'trading::placeRegularOrderBatch_native batch_reordered'),
    ('amendment-trigger-execution', 'update_order', '("triggerPrice", trigger),\n                ("executePrice", execution),', '("triggerPrice", execution),\n                ("executePrice", trigger),', 'trading::updateOrder_native update_stop'),
    ('position-existing-id-filter', 'fetch_positions', '!(r.set_has)', '(r.set_has)', 'trading::positions_native_four_ids'),
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
    result = {'control': name, 'function': function, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode, 'namedFailure': assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout}
    results.append(result)
    (out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    assert broken.returncode == 101 and result['namedFailure'], (name, result)
