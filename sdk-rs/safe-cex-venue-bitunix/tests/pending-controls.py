from pathlib import Path
import json
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitunix'
out = root / 'sdk-rs/target/bitunix-pending-evidence'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('income-sign', 'history_mappers.rs', 'income: -fee,', 'income: fee,', 'history_mappers', 'HISTORY decoded native "trade"'),
    ('closed-notional', 'history_mappers.rs', 'contracts * close_price', 'contracts + close_price', 'history_mappers', 'HISTORY decoded native "closed"'),
    ('historical-type', 'history_mappers.rs', 'OrderType::Market', 'OrderType::Limit', 'history_mappers', 'HISTORY decoded native "order"'),
    ('listener-rejection', 'websocket_public.rs', 'message: "Not implemented"', 'message: "Implemented"', 'public_listener_errors', 'PUBLIC listener native rejection'),
    ('ordered-set', 'websocket_public.rs', 'if !symbols.contains(&symbol)', 'if symbols.contains(&symbol)', 'public_subscriptions', 'SUB native full UTF16 set'),
    ('batch-default', 'websocket_public.rs', 'batch_size.unwrap_or(20.0)', 'batch_size.unwrap_or(19.0)', 'public_subscriptions', 'SUB native effect "enqueueSubscriptions"'),
    ('market-cap', 'websocket_public.rs', '.take(BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize)', '.take(BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize - 1)', 'public_subscriptions', 'SUB native event count'),
    ('position-contracts', 'position_reads.rs', 'p.contracts = contracts.abs();', 'p.contracts = contracts;', 'position_reads', 'POSITION decoded native "positions"'),
    ('order-aggregation', 'position_reads.rs', 'orders.extend(tpsl_orders);', 'orders.extend(tpsl_orders.into_iter().take(0));', 'position_reads', 'POSITION decoded native "orders"'),
    ('snapshot-positive-price', 'order_state.rs', 'if !price.is_finite() || price <= 0.0', 'if !price.is_finite() || price < 0.0', 'order_state', 'ORDER STATE decoded native "snapshot"'),
    ('position-id-precedence', 'order_state.rs', '.and_then(|i| i.get("positionId"))\n        .filter(|v| js_truthy(Some(v)))', '.and_then(|i| i.get("positionId"))\n        .filter(|v| !js_truthy(Some(v)))', 'order_state', 'ORDER STATE native position id identity'),
    ('full-position-hydration', 'order_state.rs', 'if full {', 'if full && amount > 0.0 {', 'order_state', 'ORDER STATE native hydration effect count'),
]
selected = set(sys.argv[1:])
assert not selected - {c[0] for c in controls}, 'Unknown control name'
results = []
for name, file, original_text, mutation, target, assertion in controls:
    if selected and name not in selected:
        continue
    source = venue / 'src' / file
    original = source.read_text()
    assert original_text in original, name
    command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitunix', '--jobs', '2', '--test', target, '--', '--nocapture']
    try:
        source.write_text(original.replace(original_text, mutation))
        failed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-mutated.log')).write_text(failed.stdout)
        assert failed.returncode == 101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout, (name, failed.stdout[-2500:])
    finally:
        source.write_text(original)
    passed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(passed.stdout)
    assert passed.returncode == 0, (name, passed.stdout[-2500:])
    results.append({'family': name, 'compiled_exit': failed.returncode, 'assertion': assertion, 'restored_exit': passed.returncode})
    (out / ('selected-controls.json' if selected else 'controls.json')).write_text(json.dumps(results, indent=2) + '\n')
    print(name + ': compiled native failure 101; restored pass 0', flush=True)
