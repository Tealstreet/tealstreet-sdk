#!/usr/bin/env python3
"""Compiling, sequential controls for the new Coinbase order field bodies; restore bytes even on failure."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-order-create-controls') / SHA[:10]
OUT.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'SAFE_CEX_TYPESCRIPT_PATH': '/Users/sam/cs/tealstreet-next/node_modules/typescript'}
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-coinbase', '--jobs', '2']
RESULTS = []

def run(label, selector, test, expected, diagnostic=None):
    command = CARGO + selector + [test, '--', '--exact']
    result = subprocess.run(command, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (OUT / (label + '.log')).write_text(result.stdout)
    record = {'label': label, 'code_sha': SHA, 'command': command, 'exit': result.returncode,
              'expected_exit': expected, 'diagnostic': diagnostic,
              'diagnostic_found': diagnostic is None or diagnostic in result.stdout,
              'compiling_test_failure': expected == 0 or ('test result: FAILED.' in result.stdout and 'error[E' not in result.stdout),
              'log_sha256': hashlib.sha256(result.stdout.encode()).hexdigest(),
              'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff'])).hexdigest()}
    RESULTS.append(record)
    (OUT / 'results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    if result.returncode != expected or not record['diagnostic_found'] or not record['compiling_test_failure']:
        print(result.stdout[-10000:], flush=True)
        raise RuntimeError(label + ': unexpected gate result; compiling survivors are findings')

def control(label, old, new, selector, test, diagnostic):
    path = CRATE / 'src/order_create.rs'
    original = path.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, label + ': expected one mutation call site'
    try:
        path.write_text(source.replace(old, new, 1))
        run(label + '-mutated', selector, test, 101, diagnostic)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    run(label + '-restored', selector, test, 0)

SELECT = ['--test', 'order_create']
control('amount-precision', 'let amount = adjust(opts.amount, p_amount);', 'let amount = adjust(opts.amount, p_price + 0.0 * p_amount);', SELECT,
        'native_create_payload_matches_live_source', 'native amount precision')
control('optimistic-zero-price', 'price <= 0.0', 'price < 0.0', SELECT,
        'native_optimistic_order_matches_live_source', 'decoded native optimistic presence')
control('placement-reconciliation', '.schedule_open_orders_rest_reconciliation("place-order")?', '.schedule_open_orders_rest_reconciliation("wrong-reason")?', SELECT,
        'native_place_order_matches_live_source', 'native reconciliation reason')
control('cancel-batch-size', 'ids.chunks(100)', 'ids.chunks(101)', SELECT,
        'native_cancel_order_matches_live_source', 'native side effect count')
control('poll-deadline', 'now + 10_000.0', 'now + 20_000.0', SELECT,
        'native_poll_order_matches_live_source', 'decoded native number bits')
control('rest-freshness-boundary', 'now - existing.last_update < 5_000.0', 'now - existing.last_update <= 5_000.0', SELECT,
        'native_rest_merge_matches_live_source', 'decoded native merged order count')
control('rate-limit-deadline', 'host.now_ms() + 60_000.0', 'host.now_ms() + 6_000.0', SELECT,
        'native_fetch_error_matches_live_source', 'decoded native number bits')
control('alias-preference', 'if product_id.ends_with("-USD") && alias.is_some()', 'if false && product_id.ends_with("-USD") && alias.is_some()', SELECT,
        'native_market_resolver_matches_live_source', 'decoded native alias market')
control('shallow-info-alias', '                        value.clone()', '                        Rc::new(RefCell::new(value.borrow().clone()))', SELECT,
        'native_optimistic_info_retains_shallow_aliases_and_primitive_copies', 'native optimistic info object alias')
control('dropped-producer', '-> CoinbaseStarbasePromise<Result<Vec<String>, CoinbaseOrderFailure>> {',
        '-> CoinbaseStarbasePromise<Result<Vec<String>, CoinbaseOrderFailure>> {\n    let _ = spawn;\n    let abandon: CoinbaseStarbasePromiseSpawner = Rc::new(|mut producer| { let _ = producer.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())); });\n    let spawn = &abandon;', SELECT,
        'native_unawaited_order_promises_continue_like_source', 'native unawaited producer continues through write check')
print('Ten compiling controls caught and restored byte for byte.', flush=True)
