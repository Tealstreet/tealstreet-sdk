#!/usr/bin/env python3
"""Corrupt each Exchange dispatch independently; require its E2E test to fail.

Run from any directory. Logs stay in a temporary directory; source bytes are
restored even when a control fails. These controls prove dispatch coverage,
not complete JavaScript semantics or live exchange parity.
"""
import re
import subprocess
import tempfile
from pathlib import Path

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
SOURCE = CRATE / "src/spot_binding.rs"
CARGO = [
    "cargo", "+1.99.0", "test", "--locked", "--manifest-path",
    "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-hyperliquid", "--test", "spot_binding",
]
CONTROLS = [
    ("start", "start", "bound_start_loads_spot_markets_balance_and_both_sockets"),
    ("place_order", "placeOrder", "bound_place_order_signs_shifted_asset_and_preserves_client_id"),
    ("get_account", None, "bound_get_account_preserves_base_error"),
    ("validate_account", "validateAccount", "bound_validate_account_checks_real_wallet_pair"),
    ("fetch_positions", "fetchPositions", "bound_fetch_positions_preserves_source_store_snapshot_without_writes"),
    ("fetch_orders", "fetchOrders", "bound_fetch_orders_preserves_source_empty_result_without_http"),
    ("fetch_markets", None, "bound_fetch_markets_preserves_base_error"),
    ("fetch_tickers", None, "bound_fetch_tickers_preserves_base_error"),
    ("fetch_symbol_max_leverage", None, "bound_fetch_symbol_max_leverage_preserves_base_error"),
    ("set_leverage", "setLeverage", "bound_set_leverage_dispatches_source_update"),
    ("change_position_mode", None, "bound_change_position_mode_preserves_base_error"),
    ("change_margin_type", "changeMarginType", "bound_change_margin_type_preserves_source_absent_active_leverage"),
    ("update_order", "updateOrder", "bound_update_order_cancels_then_returns_new_order_ids"),
    ("cancel_orders", "cancelOrders", "bound_cancel_orders_uses_shifted_asset"),
    ("schedule_cancel", None, "bound_schedule_cancel_preserves_base_error"),
    ("fetch_public_trades", None, "bound_fetch_public_trades_preserves_base_error"),
    ("fetch_ohlcv", "fetchOHLCV", "bound_ohlcv_reads_candles_with_source_interval_and_times"),
    ("listen_ohlcv", "listenOHLCV", "bound_public_candle_listener_dispatches_source_event_and_cleanup"),
    ("listen_order_book", "listenOrderBook", "bound_public_book_listener_dispatches_source_levels_and_cleanup"),
]


def main():
    original = SOURCE.read_bytes()
    text = original.decode()
    logs = Path(tempfile.mkdtemp(prefix="hyperliquid-spot-binding-controls-"))
    print(f"logs: {logs}", flush=True)

    def run(label, test=None):
        command = CARGO + ([test, "--", "--exact"] if test else [])
        result = subprocess.run(command, cwd=ROOT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / f"{label}.log").write_text(result.stdout)
        print(f"{label}: exit {result.returncode}", flush=True)
        return result

    baseline = run("baseline")
    assert baseline.returncode == 0 and "21 passed; 0 failed" in baseline.stdout, baseline.stdout
    try:
        for name, dispatch, test in CONTROLS:
            # Restrict mutation to this one trait override, never a helper or
            # another override. A compiling failure is mandatory below.
            pattern = rf"(    fn {name}\([^\n]*(?:\n(?!    fn ).*)*)"
            match = re.search(pattern, text)
            assert match, name
            body = match.group()
            if name == "place_order":
                match = re.search(r"    fn place_order\([\s\S]*?(?=    fn place_orders)", text)
                assert match
                body = match.group()
            old = f'"{dispatch}"' if dispatch else "base_exchange_rejection()"
            new = (f'"CONTROL_{dispatch}"' if dispatch else
                   'Box::pin(async { Err(ExchangeError::new("CONTROL rejection")) })')
            assert body.count(old) == 1, (name, body)
            SOURCE.write_text(text[:match.start()] + body.replace(old, new)
                              + text[match.end():])
            result = run(name, test)
            assert result.returncode == 101, (name, result.stdout)
            assert "error[E" not in result.stdout, (name, result.stdout)
            assert f"test {test} ... FAILED" in result.stdout, (name, result.stdout)
            assert "0 passed; 1 failed" in result.stdout, (name, result.stdout)
            SOURCE.write_bytes(original)
    finally:
        SOURCE.write_bytes(original)
    restored = run("restored")
    assert restored.returncode == 0 and "21 passed; 0 failed" in restored.stdout, restored.stdout
    assert SOURCE.read_bytes() == original
    print("19/19 compiling dispatch controls failed their named test; restored 21/21 pass", flush=True)


if __name__ == "__main__":
    main()
