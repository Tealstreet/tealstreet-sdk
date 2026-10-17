"""Prove the actual WOO journal dispatch and imported fold wiring independently."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
exchange = root / "safe-cex-venue-woo/src/binding/exchange.rs"
globals_file = root / "safe-cex-venue-woo/src/binding/globals.rs"
intervals = root / "safe-cex-exchange/src/position_intervals.rs"
originals = {p: p.read_bytes() for p in (exchange, globals_file, intervals)}
command = ["cargo", "test", "--manifest-path", str(root / "Cargo.toml"),
           "--locked", "--jobs", "1", "-p", "safe-cex-venue-woo", "--test", "binding_streams"]
name = "real_journal_folds_historical_orders_with_reported_pnl"

def run(label, selected=False, failure=False):
    result = subprocess.run(command + ([name, "--", "--exact"] if selected else []),
                            cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/woo-journal-" + label + ".log").write_text(result.stdout)
    if failure:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert "test " + name + " ... FAILED" in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and "test result: ok" in result.stdout, result.stdout
    print(label, "named failure" if failure else "passed", flush=True)

if sys.argv[1:] == ["--interval-only"]:
    command[command.index("safe-cex-venue-woo")] = "safe-cex-exchange"
    command[command.index("binding_streams")] = "journal"
    name = "position_interval_native_bridge_retains_numbers_and_absence"
    run("interval-baseline", selected=True)
    source = originals[intervals].decode()
    before = "PositionIntervalWire::deserialize(safe_cex_types::strict_decode::object(decoder))"
    assert source.count(before) == 1
    try:
        intervals.write_text(source.replace(before, "PositionIntervalWire::deserialize(decoder)"))
        run("interval-object-guard", selected=True, failure=True)
    finally:
        intervals.write_bytes(originals[intervals])
    run("interval-object-guard-restored", selected=True)
    sys.exit(0)
assert not sys.argv[1:], "supported option: --interval-only"
run("baseline")
s = originals[exchange].decode()
start = s.index("    fn fetch_historical_positions(")
end = s.index("    fn fetch_historical_orders(", start)
controls = [("trait-dispatch", exchange, s[:start] + s[end:])]
for helper in ("wooOrdersToExecutions", "reconstructPositionIntervals", "mapFoldedInterval"):
    source = originals[globals_file].decode()
    needle = '        "' + helper + '",'
    assert source.count(needle) == 1
    controls.append((helper, globals_file, source.replace(needle, needle.replace(helper, "missing" + helper))))
s = originals[intervals].decode()
needle = '#[serde(rename_all = "camelCase")]\npub struct PositionInterval'
assert s.count(needle) == 1
controls.append(("native-field-names", intervals, s.replace(needle, needle.replace("camelCase", "snake_case"))))
for label, path, mutation in controls:
    try:
        path.write_text(mutation)
        run(label, selected=True, failure=True)
    finally:
        for original, content in originals.items():
            original.write_bytes(content)
    run(label + "-restored", selected=True)
assert all(p.read_bytes() == content for p, content in originals.items())
