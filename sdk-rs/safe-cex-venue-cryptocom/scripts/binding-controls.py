"""One production mutation per bound Exchange trait method.

Each mutation rewires one method of src/binding/exchange.rs, requires its named
tests/exchange_binding.rs test to fail, restores the original bytes and requires
the same test to pass again. Run from sdk-rs/: python3 safe-cex-venue-cryptocom/scripts/binding-controls.py
"""
import re
import subprocess
import sys

SRC = "safe-cex-venue-cryptocom/src/binding/exchange.rs"
GLOBALS = "safe-cex-venue-cryptocom/src/binding/globals.rs"
# Whole-file controls for the module-scope helpers the trait methods depend on.
GLOBAL_MUTATIONS = [
    ("onRejected replay", "with_rejected_handlers(&rt, client::create(rt.shared.http.clone(), arg(&a, 0)))",
     "{ let _ = with_rejected_handlers; Ok(client::create(rt.shared.http.clone(), arg(&a, 0))) }", "a_non_2xx_signature_rejection_retries_through_axios_retry"),
    ("resendOnSignatureRejection", 'xhr.call_method("request", vec![resend])', '{ let _ = (xhr, resend); Ok(Js::Undefined) }',
     "a_signature_rejection_in_a_200_envelope_resends_once_resigned"),
    ("signatureRejectedResponse", "config.set(SIGNATURE_RETRY_CONFIG_KEY, Js::Bool(true))?;\n                Ok(Js::Bool(true))",
     "Ok(Js::Bool(false))", "a_non_2xx_signature_rejection_retries_through_axios_retry"),
]
INHERITED = "inherited_base_members_reject_or_resolve_empty"
MUTATIONS = [
    ("dispose", "self.base().dispose()?;", "", "disposal_during_fetch_markets_stops_start_before_store_update"),
    ("place_order", '"placeOrder"', '"placeOrders"', "place_order_signs_a_limit_create_order"),
    ("place_orders", '"placeOrders"', '"placeOrder"', "place_orders_places_each_order_in_series"),
    ("start", '"start"', '"fetchMarkets"', "start_loads_markets_tickers_sockets_and_the_private_snapshot"),
    ("get_account", '"getAccount"', '"validateAccount"', "get_account_reads_the_account_uuid"),
    ("validate_account", '"validateAccount"', '"getAccount"', "validate_account_returns_the_balance_on_success"),
    ("fetch_positions", '"fetchPositions"', '"fetchOrders"', "fetch_positions_maps_signed_quantity"),
    ("fetch_orders", '"fetchOrders"', '"fetchPositions"', "fetch_orders_merges_regular_and_advanced_books"),
    ("fetch_markets", '"fetchMarkets"', '"fetchTickers"', "fetch_markets_maps_perpetual_instruments"),
    ("fetch_tickers", '"fetchTickers"', '"fetchMarkets"', "fetch_tickers_keeps_perpetuals"),
    ("fetch_symbol_max_leverage", "not_implemented()", "Box::pin(async { Ok(1.0) })", INHERITED),
    ("set_leverage", '"setLeverage"', '"changeMarginType"', "set_leverage_posts_the_account_leverage"),
    ("change_position_mode", '"changePositionMode"', '"changeMarginType"', "change_position_mode_only_warns_and_change_margin_type_rejects"),
    ("change_margin_type", '"changeMarginType"', '"changePositionMode"', "change_position_mode_only_warns_and_change_margin_type_rejects"),
    ("update_order", '"updateOrder"', '"cancelOrders"', "update_order_amends_price_and_quantity"),
    ("cancel_orders", '"cancelOrders"', '"updateOrder"', "cancel_orders_routes_triggers_to_the_advanced_endpoint"),
    ("schedule_cancel", "not_implemented()", "Box::pin(async { Ok(()) })", INHERITED),
    ("fetch_public_trades", "not_implemented()", "Box::pin(async { Ok(vec![]) })", INHERITED),
    ("fetch_historical_fills", "Ok(vec![])", 'Err(ExchangeError::new("mutant"))', INHERITED),
]


def run(test):
    result = subprocess.run(
        ["cargo", "test", "--locked", "-q", "--jobs", "1", "-p", "safe-cex-venue-cryptocom", "--test", "exchange_binding", test, "--", "--exact"],
        capture_output=True,
        text=True,
    )
    return result.returncode, result.stdout + result.stderr


def mutate(original, fn, old, new):
    start = re.search(rf"    fn {fn}[(<]", original).start()
    end = original.find("\n    fn ", start + 1)
    end = len(original) if end < 0 else end
    body = original[start:end]
    if old not in body:
        raise SystemExit(f"{fn}: {old!r} is absent; the control no longer targets production code")
    return original[:start] + body.replace(old, new, 1) + original[end:]


def main():
    selected = set(sys.argv[1:])
    known = {m[0] for m in MUTATIONS + GLOBAL_MUTATIONS}
    if selected - known:
        raise SystemExit(f"unknown controls: {sorted(selected - known)}")
    mutations = [m for m in MUTATIONS if not selected or m[0] in selected]
    global_mutations = [m for m in GLOBAL_MUTATIONS if not selected or m[0] in selected]
    original = open(SRC).read()
    globals_original = open(GLOBALS).read()
    ok = True
    # Check the restored implementation before making a failure claim.
    for test in dict.fromkeys(m[3] for m in mutations + global_mutations):
        code, out = run(test)
        if code != 0 or "test result: ok. 1 passed" not in out:
            raise SystemExit(f"baseline {test}:\n{out}")
    try:
        for fn, old, new, test in mutations:
            open(SRC, "w").write(mutate(original, fn, old, new))
            code, out = run(test)
            failed = code == 101 and test in out and "FAILED" in out
            if not failed:
                print(f"{fn} mutant did not fail at runtime:\n{out}", flush=True)
            open(SRC, "w").write(original)
            code, out = run(test)
            restored = code == 0 and "test result: ok. 1 passed" in out
            ok &= failed and restored
            print(f"{fn:28} {test:66} mutant_failed={failed} restored_pass={restored}", flush=True)
        for name, old, new, test in global_mutations:
            old = old.replace("\\n", "\n")
            if old not in globals_original:
                raise SystemExit(f"{name}: {old!r} is absent")
            open(GLOBALS, "w").write(globals_original.replace(old, new, 1))
            code, out = run(test)
            failed = code == 101 and test in out and "FAILED" in out
            if not failed:
                print(f"{name} mutant did not fail at runtime:\n{out}", flush=True)
            open(GLOBALS, "w").write(globals_original)
            code, out = run(test)
            restored = code == 0 and "test result: ok. 1 passed" in out
            ok &= failed and restored
            print(f"{name:28} {test:66} mutant_failed={failed} restored_pass={restored}", flush=True)
    finally:
        open(SRC, "w").write(original)
        open(GLOBALS, "w").write(globals_original)
    sys.exit(0 if ok else 1)


main()
