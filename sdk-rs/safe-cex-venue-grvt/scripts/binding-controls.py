#!/usr/bin/env python3
"""Compiling controls for each bound Exchange entry; restore exact source bytes."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-grvt/src/binding/exchange.rs'
original = source.read_bytes()
controls = [
    ('start', 'start', 'start_public_only_loads_markets_tickers_and_opens_public_socket'),
    ('get_account', 'getAccount', 'constructor_and_account_use_trading_gateway'),
    ('validate_account', 'validateAccount', 'validate_account_seeds_sdk_cookie_and_sends_null_funding_body'),
    ('fetch_positions', 'fetchPositions', 'fetch_positions_maps_private_rows_and_preserves_source_filter_mapping'),
    ('fetch_orders', 'fetchOrders', 'fetch_orders_maps_nonce_and_singleton_source_filter'),
    ('fetch_markets', 'fetchMarkets', 'fetch_markets_uses_public_gateway_and_keeps_signing_metadata'),
    ('fetch_tickers', 'fetchTickers', 'fetch_tickers_uses_loaded_markets_without_http'),
    ('set_leverage', 'setLeverage', 'set_leverage_sends_sdk_request_and_updates_cache'),
    ('place_order', 'placeOrder', 'place_order_signs_lite_sdk_payload_and_keeps_caller_id'),
    ('place_orders', 'placeOrders', 'place_orders_runs_source_sequential_batch'),
    ('cancel_orders', 'cancelOrders', 'cancel_orders_reconciles_source_nonce_to_real_order_id'),
    ('update_order', 'updateOrder', 'update_order_validates_then_cancels_and_replaces'),
    ('fetch_symbol_max_leverage', None, 'max_leverage_preserves_inherited_refusal'),
    ('change_position_mode', None, 'position_mode_preserves_inherited_refusal'),
    ('change_margin_type', None, 'margin_type_preserves_inherited_refusal'),
    ('schedule_cancel', None, 'schedule_cancel_preserves_inherited_refusal'),
    ('fetch_public_trades', None, 'public_trades_preserves_inherited_refusal'),
]
command = ['cargo', 'test', '--locked', '--manifest-path', str(root/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-grvt', '--test', 'exchange_binding']
try:
    for method, member, witness in controls:
        text = original.decode()
        # Includes lifetime parameters without confusing place_order/place_orders.
        start = text.index('    fn ' + method + ('<' if method == 'place_orders' else '('))
        end = text.find('\n    fn ', start + 1)
        if end < 0:
            end = len(text)
        block = text[start:end]
        needle = '"' + (member if member is not None else 'Not implemented') + '"'
        assert block.count(needle) == 1, (method, block)
        mutated = block.replace(needle, '"binding-control-refusal"', 1)
        source.write_text(text[:start] + mutated + text[end:])
        result = subprocess.run(command + [witness, '--', '--exact'], cwd=root, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode == 0 or 'could not compile' in output or ('test ' + witness + ' ... FAILED') not in output:
            sys.stderr.write(output)
            raise AssertionError('control must compile and fail its named assertion: ' + method)
        source.write_bytes(original)
        print(method + ': compiling mutation failed ' + witness, flush=True)
finally:
    source.write_bytes(original)
assert source.read_bytes() == original
result = subprocess.run(command, cwd=root)
assert result.returncode == 0, 'restored production must pass every E2E test'
print(str(len(controls)) + ' compiling controls and exact production restoration passed', flush=True)
