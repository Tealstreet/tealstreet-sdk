"""Compile ticker binding faults sequentially and restore exact production bytes."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
cases = {
    "bitunix": ("sockets.rs", "native_identity", "patch.set_number(*k, *n);", "patch.set_number(*k, if n.is_nan() { 0.0 } else { *n });"),
    "bitrue": ("public_bodies.rs", "exchange_binding", "let last = h.unary_plus(close)?;", "let _original = h.unary_plus(close)?; let last = h.number(0.0);"),
    "apexomni": ("ws_public.rs", "binding", '("p", "last"),', '("mp", "last"),'),
    "weex": ("public_feed.rs", "exchange_binding", 'let last = get("c", "last")?;', 'let last = get("m", "mark")?;'),
    "phemex": ("websocket_data.rs", "exchange_binding", 'let last = parse_field(h, &input, "lastRp")?;', 'let last = parse_field(h, &input, "markRp")?;'),
    "kucoin": ("public_streams.rs", "exchange_trait", 'fallback(h, (h.get)(&data, "bestBidPrice")?, || {', 'fallback(h, (h.get)(&data, "bestAskPrice")?, || {'),
    "kucoinspot": ("public_streams.rs", "exchange_trait", 'vec!["bestBid", "bestBidPrice", "buy"],', 'vec!["bestAsk", "bestAskPrice", "sell"],'),
}
selected = sys.argv[1:] or list(cases)
assert set(selected) <= cases.keys(), "unknown venue control"
for venue in selected:
    filename, target, before, after = cases[venue]
    crate = "kucoin" if venue == "kucoinspot" else venue
    test = {
        "kucoin": "production_ticker_coercion_futures_matches_constructed_typescript",
        "kucoinspot": "production_ticker_coercion_spot_matches_constructed_typescript",
    }.get(venue, "production_ticker_coercion_matches_constructed_typescript")
    source = root / "sdk-rs" / ("safe-cex-venue-" + crate) / "src" / filename
    original = source.read_bytes()
    assert original.count(before.encode()) == 1, venue
    command = ["cargo", "test", "--jobs", "1", "-p", "safe-cex-venue-" + crate,
               "--test", target, test]
    try:
        source.write_bytes(original.replace(before.encode(), after.encode(), 1))
        red = subprocess.run(command, cwd=root / "sdk-rs", capture_output=True, text=True)
        Path("/tmp/ticker-native-" + venue + "-control.log").write_text(red.stdout + red.stderr)
        assert red.returncode == 101 and "native ticker" in red.stdout + red.stderr and "test result: FAILED" in red.stdout, red.stdout + red.stderr
        print(venue + ": compiled native assertion RED", flush=True)
    finally:
        source.write_bytes(original)
    green = subprocess.run(command, cwd=root / "sdk-rs", capture_output=True, text=True)
    Path("/tmp/ticker-native-" + venue + "-restored.log").write_text(green.stdout + green.stderr)
    assert green.returncode == 0 and "1 passed" in green.stdout, green.stdout + green.stderr
    print(venue + ": exact restoration GREEN", flush=True)
