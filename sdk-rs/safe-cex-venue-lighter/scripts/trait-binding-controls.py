"""Require compiled failures through the real native venue, then exact restoration."""
from pathlib import Path
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-lighter/src/binding/exchange.rs"
original = source.read_bytes()
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-lighter",
           "--test", "exchange_binding", "real_trait_"]
names = [
    "real_trait_candles_reach_rest_and_preserve_sorted_fields",
    "real_trait_candles_refuse_unknown_market",
    "real_trait_fills_reach_signed_history_and_keep_both_rows",
    "real_trait_private_snapshot_updates_positions_orders_and_balance",
    "real_trait_book_delivers_socket_changes_and_cleans_up",
    "real_trait_candle_listener_polls_deduplicates_and_cleans_up",
]

def run(label, failures=()):
    result = subprocess.run(command, cwd=root, env=os.environ, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/lighter-trait-control-" + label + ".log").write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert all("test " + name + " ... FAILED" in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and "6 passed" in result.stdout, result.stdout
    print(label, "named failure" if failures else "passed", flush=True)

run("baseline")
s = original.decode()
for name in ["listen_ohlcv", "listen_order_book", "fetch_ohlcv", "fetch_historical_fills",
             "fetch_and_set_private_snapshot"]:
    start = s.index("    fn " + name + "(")
    end = s.index("    fn ", start + 8)
    s = s[:start] + s[end:]
# The removed listener callers intentionally orphan this helper under -D warnings.
s = s.replace("    fn listen<T:", "    #[allow(dead_code)]\n    fn listen<T:")
controls = [("trait-defaults", s, names)]
s = original.decode()
needle = '            cleanup.call(Js::Undefined, vec![]).map(|_| ()).map_err(err)'
assert s.count(needle) == 1
controls.append(("listener-cleanup", s.replace(needle, '            let _ = cleanup; Ok(())'), names[4:]))
for member, item, failure in [("fetchHistoricalFills", "Execution", names[2]), ("fetchOHLCV", "Candle", names[0])]:
    needle = '        self.invoke("' + member + '", args!(js(&options)))'
    assert s.count(needle) == 1
    for count in (0, 1):
        replacement = '''        let params = args!(js(&options));
        Box::pin(async move {
            let rows: Vec<ITEM> = self.invoke("MEMBER", params).await?;
            Ok(rows.into_iter().take(COUNT).collect())
        })'''.replace("ITEM", item).replace("MEMBER", member).replace("COUNT", str(count))
        controls.append((member + "-take-" + str(count), s.replace(needle, replacement), [failure]))
selected = set(sys.argv[1:])
assert selected <= {label for label, *_ in controls}, "unknown control selection"
for label, mutation, failures in controls:
    if selected and label not in selected:
        continue
    try:
        source.write_text(mutation)
        run(label, failures)
    finally:
        source.write_bytes(original)
    run(label + "-restored")
assert source.read_bytes() == original
