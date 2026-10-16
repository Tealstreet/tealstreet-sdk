"""One production mutation per bound Exchange trait method.

Each mutation rewires one method of src/binding/exchange.rs, requires its named
tests/exchange_binding.rs test to fail, restores the original bytes and requires
the same test to pass again. Run from sdk-rs/: python3 safe-cex-venue-kraken/scripts/binding-controls.py
"""
import subprocess
import sys

SRC = "safe-cex-venue-kraken/src/binding/exchange.rs"
REJECT = 'self.invoke("fetchMarkets", vec![])'
INHERITED = "inherited_base_members_reject_not_implemented"
MUTATIONS = [
    ("place_order", '"placeOrder"', '"placeOrders"', "place_order_posts_a_signed_form_and_returns_the_venue_id"),
    ("start", '"start"', '"fetchMarkets"', "start_loads_markets_tickers_connects_sockets_and_ticks"),
    ("get_account", "not_implemented()", REJECT, INHERITED),
    ("validate_account", '"validateAccount"', '"fetchBalance"', "validate_account_returns_no_error_and_a_balance"),
    ("fetch_positions", '"fetchPositions"', '"fetchOrders"', "private_reads_sign_and_map_orders_positions_and_balance"),
    ("fetch_orders", '"fetchOrders"', '"fetchPositions"', "private_reads_sign_and_map_orders_positions_and_balance"),
    ("fetch_markets", '"fetchMarkets"', '"fetchTickers"', "fetch_markets_maps_perpetual_instruments"),
    ("fetch_tickers", '"fetchTickers"', '"fetchMarkets"', "fetch_tickers_reads_the_rest_snapshot_for_known_markets"),
    ("fetch_symbol_max_leverage", "not_implemented()", REJECT, INHERITED),
    ("set_leverage", '"setLeverage"', '"changeMarginType"', "set_leverage_puts_a_clamped_preference"),
    ("change_position_mode", "not_implemented()", REJECT, INHERITED),
    ("change_margin_type", '"changeMarginType"', '"setLeverage"', "change_margin_type_to_isolated_sets_a_leverage_preference"),
    ("update_order", '"updateOrder"', '"cancelOrders"', "update_order_edits_a_limit_order"),
    ("cancel_orders", '"cancelOrders"', '"updateOrder"', "cancel_orders_posts_one_cancel_for_a_single_order"),
    ("schedule_cancel", "not_implemented()", REJECT, INHERITED),
    ("fetch_public_trades", "not_implemented()", REJECT, INHERITED),
]


def run(test):
    result = subprocess.run(
        ["cargo", "test", "--locked", "-q", "-p", "safe-cex-venue-kraken", "--test", "exchange_binding", test],
        capture_output=True,
        text=True,
    )
    return result.returncode, result.stdout + result.stderr


def mutate(original, fn, old, new):
    start = original.index(f"    fn {fn}(")
    end = original.find("\n    fn ", start + 1)
    end = len(original) if end < 0 else end
    body = original[start:end]
    if old not in body:
        raise SystemExit(f"{fn}: {old!r} is absent; the control no longer targets production code")
    return original[:start] + body.replace(old, new, 1) + original[end:]


def main():
    original = open(SRC).read()
    ok = True
    try:
        for fn, old, new, test in MUTATIONS:
            open(SRC, "w").write(mutate(original, fn, old, new))
            code, out = run(test)
            failed = code == 101 and test in out and "FAILED" in out
            open(SRC, "w").write(original)
            code, out = run(test)
            restored = code == 0 and "test result: ok. 1 passed" in out
            ok &= failed and restored
            print(f"{fn:28} {test:66} mutant_failed={failed} restored_pass={restored}", flush=True)
    finally:
        open(SRC, "w").write(original)
    sys.exit(0 if ok else 1)


main()
