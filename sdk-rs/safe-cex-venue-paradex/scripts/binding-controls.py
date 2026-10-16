#!/usr/bin/env python3
"""Disable each bound native hook separately; require compiled behavioral failure."""
from pathlib import Path
import argparse
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
VENUE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
SOURCE = VENUE / 'src/native_exchange.rs'
SOCKETS = VENUE / 'src/binding/sockets.rs'
MEMBERS = VENUE / 'src/binding/members.rs'
EXECUTION = VENUE / 'src/order_execution.rs'
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-paradex', '--test', 'exchange_trait', '--locked', '--jobs', '1']
SIXTEEN = 'all_sixteen_exchange_entrypoints_execute_source_workflows'
AUTH = 'private_signing_failure_sends_no_auth_or_subscribe_and_recovers'
PRECISION = 'amendment_precision_keeps_amount_execution_and_trigger_axes_distinct'
DISPOSAL = 'private_start_signs_jwt_and_subscribes_only_after_the_matching_auth_reply'
METHODS = ['start', 'get_account', 'validate_account', 'fetch_positions', 'fetch_orders',
           'fetch_symbol_max_leverage', 'set_leverage', 'change_position_mode', 'change_margin_type',
           'place_order', 'place_orders', 'update_order', 'cancel_orders', 'schedule_cancel',
           'fetch_public_trades', 'fetch_ohlcv']
assert len(METHODS) == 16
args = argparse.ArgumentParser()
args.add_argument('--auth-only', action='store_true')
options = args.parse_args()
LOGS = Path(os.environ.get('PARADEX_CONTROL_LOG_DIR') or tempfile.mkdtemp(prefix='paradex-binding-controls-'))
LOGS.mkdir(parents=True, exist_ok=True)
original = SOURCE.read_bytes()
original_sockets = SOCKETS.read_bytes()
original_members = MEMBERS.read_bytes()
original_execution = EXECUTION.read_bytes()

def run(label, witness, expected):
    result = subprocess.run([*COMMAND, witness, '--', '--exact'], cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert result.returncode == expected, f'{label}: exit {result.returncode}, expected {expected}; {LOGS}'
    assert '1 test' in output and '0 tests' not in output, f'{label}: witness must execute'
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output, f'{label}: must compile'
        assert witness in output and 'panicked at' in output, f'{label}: named behavioral failure required'
    print(f'{label}: exit {result.returncode}', flush=True)

try:
    if not options.auth_only:
        run('baseline', SIXTEEN, 0)
        for method in METHODS:
            text = original.decode()
            # Only production Exchange override declarations use this indentation.
            token = f'    fn {method}'
            start = text.index(token, text.index('impl Exchange for ParadexExchange'))
            if method == 'place_order':
                # Preserve the required client-ID check as the first statement.
                insertion = text.index('        self.invoke("placeOrder"', start)
            else:
                insertion = text.index('{', start) + 1
            injected = f'\n        if std::hint::black_box(true) {{ return Box::pin(async {{ Err(ExchangeError::new("control:{method}")) }}); }}\n'
            SOURCE.write_text(text[:insertion] + injected + text[insertion:])
            try:
                run(method, SIXTEEN, 101)
            finally:
                SOURCE.write_bytes(original)
            run(f'{method}-restored', SIXTEEN, 0)
        run('precision-baseline', PRECISION, 0)
        text = original_execution.decode()
        start = text.index('pub async fn update_order')
        prefix, region = text[:start], text[start:]
        amount = 'h.get(&h.get(&m, "precision")?, "amount")?'
        price = 'h.get(&h.get(&m, "precision")?, "price")?'
        assert region.count(amount) == 1 and region.count(price) == 2
        axes = [
            ('amend-amount-axis', amount, price, 0),
            ('amend-execution-axis', price, amount, 0),
            ('amend-trigger-axis', price, amount, 1),
        ]
        for name, old, new, occurrence in axes:
            position = -1
            for _ in range(occurrence + 1):
                position = region.index(old, position + 1)
            EXECUTION.write_text(prefix + region[:position] + new + region[position + len(old):])
            try:
                run(name, PRECISION, 101)
                assert 'PARADEX PRECISION' in (LOGS / f'{name}.log').read_text()
            finally:
                EXECUTION.write_bytes(original_execution)
            run(f'{name}-restored', PRECISION, 0)
        run('disposal-baseline', DISPOSAL, 0)
        text = original.decode()
        old = '        self.rt.this.call_method("dispose", vec![])?;'
        assert text.count(old) == 1
        SOURCE.write_text(text.replace(old, '        self.rt.base().dispose()?;\n' + old))
        try:
            run('disposal-extra-base', DISPOSAL, 101)
            assert 'PARADEX DISPOSAL source retains exchange listeners' in (LOGS / 'disposal-extra-base.log').read_text()
        finally:
            SOURCE.write_bytes(original)
        run('disposal-extra-base-restored', DISPOSAL, 0)
        text = original_members.decode()
        old = 'rt.this.set("isDisposed", Js::Bool(flag.get()))?;'
        assert text.count(old) == 1
        MEMBERS.write_text(text.replace(old, 'rt.this.define("isDisposed", Js::Bool(flag.get()))?;'))
        try:
            run('disposal-shadow-flag', DISPOSAL, 101)
            assert 'PARADEX DISPOSAL source writes the shared flag' in (LOGS / 'disposal-shadow-flag.log').read_text()
        finally:
            MEMBERS.write_bytes(original_members)
        run('disposal-shadow-flag-restored', DISPOSAL, 0)
    run('auth-baseline', AUTH, 0)
    text = original_sockets.decode()
    old = '*observer.borrow_mut() = Some(error);'
    assert text.count(old) == 1
    SOCKETS.write_text(text.replace(old, '*observer.borrow_mut() = None; let _ = error;'))
    try:
        run('auth-recovery-observer', AUTH, 101)
        assert 'PARADEX AUTH FAILURE shared recovery must execute' in (LOGS / 'auth-recovery-observer.log').read_text()
    finally:
        SOCKETS.write_bytes(original_sockets)
    run('auth-recovery-restored', AUTH, 0)
finally:
    SOURCE.write_bytes(original)
    SOCKETS.write_bytes(original_sockets)
    MEMBERS.write_bytes(original_members)
    EXECUTION.write_bytes(original_execution)
print(f'Compiled controls rejected; exact restorations pass; logs {LOGS}')
