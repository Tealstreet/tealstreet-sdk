"""Compile real production binding faults; require named failures and exact restoration."""
from pathlib import Path
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
venue = root / "safe-cex-venue-paradex/src"
native = venue / "native_exchange.rs"
members = venue / "binding/members.rs"
host = venue / "binding/host.rs"
sockets = venue / "binding/sockets.rs"
originals = {p: p.read_bytes() for p in (native, members, host, sockets)}
env = dict(os.environ)
env.setdefault("SAFE_CEX_TYPESCRIPT_PATH", str(root.parent / "node_modules/typescript"))
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-paradex",
           "--test", "exchange_trait", "real_trait_"]
all_names = [
    "real_trait_history_fills_reach_authenticated_rows",
    "real_trait_history_income_reaches_fill_ledger",
    "real_trait_history_income_range_keeps_source_filter",
    "real_trait_history_orders_reach_authenticated_rows",
    "real_trait_history_journal_uses_symbol_scoped_fills",
    "real_trait_stream_book_delivers_and_cleans_up_socket_listener",
    "real_trait_stream_ohlcv_polls_then_stops_after_cleanup",
]

def run(label, failures=()):
    result = subprocess.run(command, cwd=root, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/paradex-trait-control-" + label + ".log").write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert all("test " + name + " ... FAILED" in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and "7 passed" in result.stdout, result.stdout
    print(label, "named failure" if failures else "passed", flush=True)

run("baseline")
s = originals[native].decode()
for name in ["listen_ohlcv", "listen_order_book", "fetch_historical_fills", "fetch_income_history",
             "fetch_income_history_range", "fetch_historical_orders", "fetch_historical_positions"]:
    start = s.index("    fn " + name + "(")
    end = s.index("    fn ", start + 8)
    s = s[:start] + s[end:]
# Removing the only callers intentionally orphans these helpers; allow only that
# mutation artifact so the behavioral control must still compile under -D warnings.
s = s.replace("    fn listen<T:", "    #[allow(dead_code)]\n    fn listen<T:")
s = s.replace("struct NativeJournal;", "#[allow(dead_code)]\nstruct NativeJournal;")
controls = [("trait-defaults", native, s, all_names)]
s = originals[members].decode()
for name in ["fetchHistoricalFills", "fetchHistoricalOrders", "fetchIncomeHistory", "fetchIncomeHistoryRange"]:
    needle = '        "' + name + '" =>'
    assert s.count(needle) == 1
    s = s.replace(needle, needle.replace(name, "missing" + name))
controls.append(("history-dispatch", members, s, all_names[:5]))
s = originals[native].decode()
start = s.index("                fetch_historical_fills: CapabilityBinding")
end = s.index("                fetch_income_history:", start)
s = s[:start] + "                fetch_historical_fills: ExchangeCapabilities::default().fetch_historical_fills,\n" + s[end:]
controls.append(("fills-brand", native, s, [all_names[4]]))
s = originals[native].decode()
needle = "                    scoped,"
assert s.count(needle) == 1
controls.append(("journal-scope", native, s.replace(needle, "                    !scoped,"), [all_names[4]]))
s = originals[host].decode()
needle = '.get("orderbookCallbacks")?'
assert s.count(needle) == 1
controls.append(("book-callback-field", host, s.replace(needle, '.get("orderBookCallbacks")?'), [all_names[5]]))
s = originals[sockets].decode()
needle = '        "ohlcv-cleanup" => {'
start = s.index(needle)
end = s.index('        "ohlcv-slippage-error"', start)
controls.append(("candle-cleanup", sockets, s[:start] + '        "ohlcv-cleanup" => return Ok(Some(Js::Undefined)),\n' + s[end:], [all_names[6]]))
s = originals[members].decode()
needle = '        "fetchHistoricalFills" => crate::account_rest::fetch_historical_fills(&h, x).await,'
assert s.count(needle) == 1
for count in (0, 1):
    replacement = '''        "fetchHistoricalFills" => {
            let rows = crate::account_rest::fetch_historical_fills(&h, x).await?;
            Ok(Js::array(rows.array_values().expect("native history array").into_iter().take(COUNT).collect()))
        },'''.replace("COUNT", str(count))
    controls.append(("fills-take-" + str(count), members, s.replace(needle, replacement), [all_names[0]]))
selected = set(sys.argv[1:])
assert selected <= {label for label, *_ in controls}, "unknown control selection"
for label, path, mutation, failures in controls:
    if selected and label not in selected:
        continue
    try:
        path.write_text(mutation)
        run(label, failures)
    finally:
        for original, content in originals.items():
            original.write_bytes(content)
    run(label + "-restored")
assert all(path.read_bytes() == original for path, original in originals.items())
