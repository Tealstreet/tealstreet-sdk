"""Prove every typed Exchange entrypoint reaches its concrete source binding.

Run sequentially after the exchange_binding target is green. Every mutation
compiles, fails the named E2E assertion, and is restored before the next control.
Source-body algorithm controls live alongside their existing venue oracles.
"""
from pathlib import Path
import os
import re
import subprocess
import tempfile

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / "src/binding/exchange.rs"
original = source.read_text()
cases = [
    ("place_order", "concrete_place_order_posts_real_signed_gtc_payload"),
    ("start", "concrete_markets_and_public_start"),
    ("get_account", "concrete_wallet_account_matches_ethers_address"),
    ("validate_account", "concrete_validate_derives_real_l1_signature_and_l2_headers"),
    ("fetch_positions", "concrete_fetch_positions_uses_holdings_and_keeps_snapshot_pure"),
    ("fetch_orders", "concrete_fetch_orders_maps_all_pages_without_store_commit"),
    ("fetch_markets", "concrete_markets_and_public_start"),
    ("fetch_tickers", "concrete_markets_and_public_start"),
    ("fetch_symbol_max_leverage", "concrete_constructor_account_and_source_controls"),
    ("set_leverage", "concrete_constructor_account_and_source_controls"),
    ("change_position_mode", "concrete_constructor_account_and_source_controls"),
    ("change_margin_type", "concrete_constructor_account_and_source_controls"),
    ("update_order", "concrete_update_order_cancels_then_signs_replacement"),
    ("cancel_orders", "concrete_cancel_orders_batches_only_ids"),
    ("schedule_cancel", "concrete_inherited_rejections_match_source"),
    ("fetch_public_trades", "concrete_inherited_rejections_match_source"),
]
assert len(cases) == 16
command = [os.environ.get("CARGO", "cargo"), "test", "--manifest-path", str(sdk / "Cargo.toml"), "-p", "safe-cex-venue-polymarket", "--locked", "--jobs", "1", "--test", "exchange_binding"]
directory = Path(os.environ.get("POLYMARKET_CONTROL_LOG_DIR") or tempfile.mkdtemp(prefix="polymarket-binding-controls-"))
directory.mkdir(parents=True, exist_ok=True)


def run(name, test=None):
    args = command + ([test, "--", "--exact", "--nocapture"] if test else [])
    result = subprocess.run(args, capture_output=True, text=True)
    (directory / (name + ".txt")).write_text(result.stdout + result.stderr)
    return result.returncode, result.stdout + result.stderr


code, _ = run("baseline")
assert code == 0, f"E2E baseline must pass; {directory}"
for method, test in cases:
    start = original.index("    fn " + method + "(", original.index("impl Exchange for"))
    body = original.index("{", start)
    depth = 1
    end = body + 1
    while depth:
        depth += (original[end] == "{") - (original[end] == "}")
        end += 1
    # Keep every argument used so this remains an executable control, never a
    # warnings-as-errors or compile-failure substitute for behavioral proof.
    arguments = re.findall(r"\b([A-Za-z_]\w*)\s*:", original[start:body])
    consume = "".join("let _ = " + argument + ";" for argument in arguments)
    replacement = '{' + consume + 'Box::pin(async { Err(ExchangeError::new("binding-control:' + method + '")) })}'
    try:
        source.write_text(original[:body] + replacement + original[end:])
        code, output = run(method + "-fail", test)
        assert code == 101 and "panicked" in output and "error[E" not in output and test + " ... FAILED" in output, f"{method} must compile and fail its named E2E; {directory}"
    finally:
        source.write_text(original)
    code, _ = run(method + "-restored", test)
    assert code == 0, f"{method} restoration failed; {directory}"
    print(method + ": compiled fail + restored pass", flush=True)
print(f"16 binding controls passed; logs={directory}")
