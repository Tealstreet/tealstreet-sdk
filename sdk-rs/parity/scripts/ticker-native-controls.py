"""Compile ticker binding faults sequentially and restore exact production bytes."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
cases = {
    "asterdex": ("public_stream.rs", "exchange_trait", '("last", update.patch.last),', '("last", update.patch.volume),'),
    "asterdex-refusal": ("public_stream.rs", "exchange_trait", '                    return Ok(None);\n                }\n                _ =>', '                    0.0\n                }\n                _ =>'),
    "orderly": ("stream_reports.rs", "exchange_trait", '("last", get(h, &row, "close")?),', '("last", get(h, &row, "amount")?),'),
    "orderly-refusal": ("binding/primitives.rs", "exchange_trait", 'if !matches!(&**value, Js::Number(_)) {', 'if false && !matches!(&**value, Js::Number(_)) {'),
    "orderly-report": ("binding/primitives.rs", "exchange_trait", 'Orderly ticker {} typed field {field} requires a number; received {}', 'Orderly ticker WRONG typed field {field} requires a number; received {} {}'),
    "woo": ("public_socket.rs", "binding_streams", '("last", "close"),', '("last", "amount"),'),
    "woo-refusal": ("public_socket.rs", "binding_streams", "                    continue 'rows;", "                    if fields.is_empty() { continue 'rows; } 0.0"),
    "bingx": ("public_stream.rs", "binding", '("mark", "fairPrice"),', '("mark", "indexPrice"),'),
    "bitget": ("ticker.rs", "exchange_binding", '("bid", &["bidPr", "bidPrice", "bid"][..]),', '("bid", &["askPr", "askPrice", "ask"][..]),'),
    "grvt-store": ("binding/public.rs", "exchange_binding", 'vec![Js::object(vec![("id", id.clone())]), patch],', 'vec![id.clone(), patch],'),
    "grvt-candle": ("public_feeds.rs", "exchange_binding", 'last: Some(close),', 'last: Some(close + 1.0),'),
    "grvt-mark": ("public_feeds.rs", "exchange_binding", 'changes.last = Some(mark);', 'changes.last = Some(mark + 1.0);'),
    "grvt": ("public_feeds.rs", "exchange_binding", '("mp", "mark_price", &mut changes.mark),', '("ip", "index_price", &mut changes.mark),'),
    "kraken": ("stream_updates.rs", "exchange_binding", 'let bid = keyed(host, message, &["bid", "best_bid"])?;', 'let bid = keyed(host, message, &["ask", "best_ask"])?;'),
    "krakenspot": ("stream_updates.rs", "exchange_binding", 'let last = optional(host, &item, "last", Lookup::Direct)?;', 'let last = optional(host, &item, "ask", Lookup::Direct)?;'),
    "mexc": ("ws_public.rs", "exchange_trait", 'patch.set_number("last", next.last);', 'patch.set_number("last", next.bid);'),
    "okx": ("public_socket.rs", "exchange_binding", 'let bid = safe_cex_num::parse_float(&js_string(update.get("bidPx")).map_err(coercion)?);', 'let bid = safe_cex_num::parse_float(&js_string(update.get("askPx")).map_err(coercion)?);'),
    "backpack": ("socket_events.rs", "exchange_trait", 'let last = number(&host.field_text(data, "c")?);', 'let last = number(&host.field_text(data, "v")?);'),
    "paradex": ("public_ingress.rs", "exchange_trait", 'let bid = h.parse_float(&first(h, &raw, &["bid_price"], "0")?)?;', 'let bid = h.parse_float(&first(h, &raw, &["ask_price"], "0")?)?;'),
    "paradex-book": ("market_data.rs", "exchange_trait", 'let bid = parsed(h, h.get(&h.get(&sorted_bids, "0")?, "price")?)?;', 'let bid = parsed(h, h.get(&h.get(&sorted_bids, "1")?, "price")?)?;'),
    "polymarket-store": ("binding/socket.rs", "exchange_binding", 'Js::object(vec![("id", id.clone())]),', 'id.clone(),'),
    "polymarket": ("ws_dispatch.rs", "exchange_binding", 'let last = safe_cex_num::parse_float(&String::from_utf16_lossy(&price));', 'let last = safe_cex_num::parse_float(&String::from_utf16_lossy(&price)) + 1.0;'),
    "bitunix": ("sockets.rs", "native_identity", "patch.set_number(*k, *n);", "patch.set_number(*k, if n.is_nan() { 0.0 } else { *n });"),
    "bitrue": ("public_bodies.rs", "exchange_binding", "let last = h.unary_plus(close)?;", "let _original = h.unary_plus(close)?; let last = h.number(0.0);"),
    "apexomni": ("ws_public.rs", "binding", '("p", "last"),', '("mp", "last"),'),
    "weex": ("public_feed.rs", "exchange_binding", 'let last = get("c", "last")?;', 'let last = get("m", "mark")?;'),
    "phemex": ("websocket_data.rs", "exchange_binding", 'let last = parse_field(h, &input, "lastRp")?;', 'let last = parse_field(h, &input, "markRp")?;'),
    "kucoin": ("public_streams.rs", "exchange_trait", 'fallback(h, (h.get)(&data, "bestBidPrice")?, || {', 'fallback(h, (h.get)(&data, "bestAskPrice")?, || {'),
    "kucoinspot": ("public_streams.rs", "exchange_trait", 'vec!["bestBid", "bestBidPrice", "buy"],', 'vec!["bestAsk", "bestAskPrice", "sell"],'),
    "hyperliquid": ("private_message_dispatch.rs", "binding", 'h.set(&buffer, "midPrice", &last)?;', 'h.set(&buffer, "mark", &last)?;'),
    "hyperliquidspot": ("spot_ws_dispatch.rs", "spot_binding", 'h.set(&buffer, "midPrice", &last)?;', 'h.set(&buffer, "mark", &last)?;'),
    "lighter": ("public_ingress.rs", "exchange_binding", 'let mark = parse_fields(&h, &data, &["markPrice", "mark_price", "mark"])?;', 'let mark = parse_fields(&h, &data, &["indexPrice", "index_price", "index"])?;'),
    "blofin": ("public_stream_data.rs", "native_ping_latency", 'let last = float(update.get("last"))?;', 'let last = float(update.get("bidPrice"))?;'),
    "cryptocom": ("socket_handlers.rs", "exchange_binding", '("last", parsed_field(h, &data, "a")?),', '("last", parsed_field(h, &data, "b")?),'),
    "coinbase": ("public_stream_dispatch.rs", "exchange_binding", '"No handler for channel ".encode_utf16()', '"Unexpected ticker channel ".encode_utf16()'),
    "binance": ("public_classes.rs", "private_classes", 'patch.set_number(field.name(), value)', 'patch.set_number(field.name(), if field.name() == "last" { 0.0 } else { value })'),
}
selected = sys.argv[1:] or list(cases)
assert set(selected) <= cases.keys(), "unknown venue control"
for venue in selected:
    filename, target, before, after = cases[venue]
    crate = {"asterdex-refusal": "asterdex", "orderly-refusal": "orderly", "orderly-report": "orderly", "woo-refusal": "woo", "kucoinspot": "kucoin", "hyperliquidspot": "hyperliquid", "polymarket-store": "polymarket", "grvt-store": "grvt", "grvt-candle": "grvt", "grvt-mark": "grvt", "paradex-book": "paradex", "krakenspot": "kraken"}.get(venue, venue)
    test = {
        "kraken": "production_ticker_coercion_futures_matches_constructed_typescript",
        "krakenspot": "production_ticker_coercion_spot_matches_constructed_typescript",
        "binance": "production_ticker_coercion_",
        "kucoin": "production_ticker_coercion_futures_matches_constructed_typescript",
        "kucoinspot": "production_ticker_coercion_spot_matches_constructed_typescript",
    }.get(venue, "production_ticker_coercion_matches_constructed_typescript")
    source = root / "sdk-rs" / ("safe-cex-venue-" + crate) / "src" / filename
    original = source.read_bytes()
    assert original.count(before.encode()) == 1, venue
    command = ["cargo", "test", "--jobs", "1", "-p", "safe-cex-venue-" + crate,
               "--test", target, test]
    expected = 3 if venue == "binance" else 1
    if venue == "binance":
        command += ["--features", "native"]
    try:
        source.write_bytes(original.replace(before.encode(), after.encode(), 1))
        red = subprocess.run(command, cwd=root / "sdk-rs", capture_output=True, text=True)
        Path("/tmp/ticker-native-" + venue + "-control.log").write_text(red.stdout + red.stderr)
        assert red.returncode == 101 and "native ticker" in red.stdout + red.stderr and "test result: FAILED" in red.stdout, red.stdout + red.stderr
        assert str(expected) + " failed" in red.stdout, red.stdout
        print(venue + ": compiled native assertion RED", flush=True)
    finally:
        source.write_bytes(original)
    green = subprocess.run(command, cwd=root / "sdk-rs", capture_output=True, text=True)
    Path("/tmp/ticker-native-" + venue + "-restored.log").write_text(green.stdout + green.stderr)
    assert green.returncode == 0 and str(expected) + " passed" in green.stdout, green.stdout + green.stderr
    print(venue + ": exact restoration GREEN", flush=True)
