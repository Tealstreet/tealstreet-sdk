"""Compile one production mutation per account-read family, then restore and retest."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/account_reads.rs'
out = root / 'sdk-rs/target/weex-account-reads-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '1', '--test', 'account_reads']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('max-default', 'fetch_symbol_max_leverage', '(rt.to_number)(host, &max, 1.0)?', '(rt.to_number)(host, &max, 2.0)?', 'native_result::max_default'),
    ('leverage-side', 'fetch_leverage_for_symbol', '"isolatedLongLeverage"', '"isolatedShortLeverage"', 'native_result::leverage_isolated'),
    ('balance-position-fields', 'fetch_balance_and_positions', '("balance", balance),\n            ("positions", positions)', '("balance", positions),\n            ("positions", balance)', 'native_result::balance_positions'),
    ('config-default', 'fetch_symbol_configs', '(rt.to_number)(host, &short, 20.0)?', '(rt.to_number)(host, &short, 21.0)?', 'native_result::configs_rows'),
    ('algo-page', 'fetch_open_order_rows', '                1.0\n', '                2.0\n', 'native_requests::rows_algo_page'),
    ('normal-filter', 'fetch_normal_orders', '.filter(rt.truthy)', '.filter(|_| true)', 'native_result::normal_orders'),
    ('algo-hole', 'fetch_algo_orders', 'if (rt.present)(host, &normalized, index)?', 'if true', 'native_result::algo_sparse_flatten'),
    ('order-source-order', 'fetch_orders', 'for orders in [normal, algo].into_iter().flatten()', 'for orders in [algo, normal].into_iter().flatten()', 'native_result::orders_unique'),
]
selected = os.environ.get('WEEX_ACCOUNT_READ_CONTROL')
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
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    result = {'control': name, 'function': function, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode, 'namedFailure': assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout}
    results.append(result)
    (out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert broken.returncode == 101 and result['namedFailure'], (name, result)
(out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
