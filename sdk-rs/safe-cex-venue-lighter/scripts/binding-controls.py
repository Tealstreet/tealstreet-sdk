"""Prove each production Exchange binding is observed by its trait E2E test.

Run without concurrent edits/Cargo processes in this worktree:
    python3 sdk-rs/safe-cex-venue-lighter/scripts/binding-controls.py
Each compiling mutation must fail the named test; exact source bytes are restored
in finally and the same test must then pass. Logs stay outside the repository.
"""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "safe-cex-venue-lighter/src/binding/exchange.rs"
INHERITED = "inherited_base_methods_reject_without_a_request"
MUTATIONS = [
    ("place_order", '\"placeOrder\"', '\"placeOrders\"', "place_order_signs_an_attributed_create_order"),
    ("place_orders", '\"placeOrders\"', '\"placeOrder\"', "place_orders_places_a_single_order_batch"),
    ("start", '\"start\"', '\"dispose\"', "start_dials_both_sockets_and_subscribes"),
    ("get_account", '\"getAccount\"', '\"fetchMarkets\"', "get_account_resolves_the_application_id_account"),
    ("validate_account", '\"validateAccount\"', '\"getAccount\"', "validate_account_looks_up_the_account_and_warms_the_signer"),
    ("fetch_positions", '\"fetchPositions\"', '\"fetchOrders\"', "fetch_positions_maps_the_account_snapshot_and_placeholders"),
    ("fetch_orders", '\"fetchOrders\"', '\"fetchPositions\"', "fetch_orders_reads_all_markets_with_a_signed_auth_token"),
    ("fetch_markets", '\"fetchMarkets\"', '\"fetchTickers\"', "fetch_markets_maps_perps_and_drops_spot_rows"),
    ("set_leverage", '\"setLeverage\"', '\"changeMarginType\"', "set_leverage_signs_an_update_leverage_transaction"),
    ("change_margin_type", '\"changeMarginType\"', '\"setLeverage\"', "change_margin_type_signs_isolated_mode_for_the_symbol"),
    ("update_order", '\"updateOrder\"', '\"cancelOrders\"', "update_order_cancels_then_replaces_at_the_new_price"),
    ("cancel_orders", '\"cancelOrders\"', '\"updateOrder\"', "cancel_orders_signs_a_cancel_for_the_order_index"),
]
for method in ["fetch_tickers", "fetch_symbol_max_leverage", "change_position_mode", "schedule_cancel", "fetch_public_trades"]:
    MUTATIONS.append((method, "inherited_rejection()", 'Box::pin(async { Err(ExchangeError::new("control")) })', INHERITED))


def run(test):
    result = subprocess.run([
        "cargo", "test", "--locked", "-q", "-p", "safe-cex-venue-lighter",
        "--test", "exchange_binding", test, "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def mutate(original, method, old, new):
    match = re.search(rb"    fn " + method.encode() + rb"(?:[<(])", original)
    if match is None:
        raise RuntimeError(f"{method}: production method absent")
    start = match.start()
    end = original.find(b"\n    fn ", start + 1)
    end = len(original) if end < 0 else end
    body = original[start:end]
    if body.count(old.encode()) != 1:
        raise RuntimeError(f"{method}: expected one production call {old}")
    return original[:start] + body.replace(old.encode(), new.encode(), 1) + original[end:]


def main():
    original = SRC.read_bytes()
    for method, old, new, test in MUTATIONS:
        code, output = run(test)
        if code or "1 passed" not in output:
            raise RuntimeError(f"{method}: baseline must pass\n{output}")
        try:
            SRC.write_bytes(mutate(original, method, old, new))
            code, output = run(test)
            if code != 101 or f"---- {test} stdout ----" not in output or "could not compile" in output:
                raise RuntimeError(f"{method}: expected a compiling mutant and named failed test\n{output}")
        finally:
            SRC.write_bytes(original)
        code, output = run(test)
        if code or "1 passed" not in output:
            raise RuntimeError(f"{method}: restored test must pass\n{output}")
        print(f"{method}: mutant_failed=true restored_pass=true", flush=True)
    print(f"{len(MUTATIONS)} bindings proved", flush=True)


if __name__ == "__main__":
    main()
