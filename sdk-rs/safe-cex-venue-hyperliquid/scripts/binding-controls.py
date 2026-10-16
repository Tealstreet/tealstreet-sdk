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
SOURCE = CRATE / "src/binding.rs"
CARGO = [
    "cargo", "+1.99.0", "test", "--locked", "--manifest-path",
    "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-hyperliquid", "--test", "binding",
]
INHERITED_TEST = "inherited_base_methods_reject_without_a_request"
CONTROLS = [
    ("start", "start", "start_loads_markets_balance_and_dials_both_sockets"),
    ("place_order", "placeOrder", "place_order_signs_a_cloid_order_and_returns_the_resting_oid"),
    ("get_account", None, INHERITED_TEST),
    ("validate_account", "validateAccount", "validate_account_checks_the_key_wallet_pair_without_a_request"),
    ("fetch_positions", "fetchPositions", "fetch_positions_returns_core_positions"),
    ("fetch_orders", "fetchOrders", "fetch_orders_reads_frontend_open_orders"),
    ("fetch_markets", None, INHERITED_TEST),
    ("fetch_tickers", None, INHERITED_TEST),
    ("fetch_symbol_max_leverage", None, INHERITED_TEST),
    ("set_leverage", "setLeverage", "set_leverage_and_margin_type_post_update_leverage"),
    ("change_position_mode", None, INHERITED_TEST),
    ("change_margin_type", "changeMarginType", "set_leverage_and_margin_type_post_update_leverage"),
    ("update_order", "updateOrder", "update_order_cancels_then_places_under_a_fresh_cloid"),
    ("cancel_orders", "cancelOrders", "cancel_orders_posts_one_cancel_per_order"),
    ("schedule_cancel", "scheduleCancel", "schedule_cancel_arms_and_clears"),
    ("fetch_public_trades", None, INHERITED_TEST),
]


def main():
    original = SOURCE.read_bytes()
    text = original.decode()
    logs = Path(tempfile.mkdtemp(prefix="hyperliquid-binding-controls-"))
    print(f"logs: {logs}", flush=True)

    def run(label, test=None):
        command = CARGO + ([test, "--", "--exact"] if test else [])
        result = subprocess.run(command, cwd=ROOT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / f"{label}.log").write_text(result.stdout)
        print(f"{label}: exit {result.returncode}", flush=True)
        return result

    baseline = run("baseline")
    assert baseline.returncode == 0 and "11 passed; 0 failed" in baseline.stdout, baseline.stdout
    try:
        for name, dispatch, test in CONTROLS:
            # Restrict mutation to this one trait override, never a helper or
            # another override. A compiling failure is mandatory below.
            pattern = rf"(    fn {name}\([^\n]*(?:\n(?!    fn ).*)*)"
            match = re.search(pattern, text)
            assert match, name
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
    assert restored.returncode == 0 and "11 passed; 0 failed" in restored.stdout, restored.stdout
    assert SOURCE.read_bytes() == original
    print("16/16 compiling dispatch controls failed their named test; restored 11/11 pass", flush=True)


if __name__ == "__main__":
    main()
