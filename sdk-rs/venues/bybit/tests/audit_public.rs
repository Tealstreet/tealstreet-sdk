//! Snapshot recovery after a documented V5 orderbook sequence gap.
mod support;

use safe_cex_engine::{Command, Event, wire};
use support::Harness;
use support::Options;

const SNAPSHOT: &str = r#"{"topic":"orderbook.1000.BTCUSDT","type":"snapshot","ts":1687940967465,"data":{"s":"BTCUSDT","b":[["30247.20","1.000"]],"a":[["30248.70","0.250"]],"u":177400506,"seq":66544703341},"cts":1687940967464}"#;

fn snapshot_books(h: &mut Harness) -> Vec<Vec<u8>> {
    wire::sections(h.engine.snapshot_frame(h.now))
        .filter(|(kind, _)| *kind == wire::section::BOOK)
        .map(|(_, payload)| payload.to_vec())
        .collect()
}

#[test]
fn missed_gap_clear_is_recoverable_from_snapshot_then_fresh_depth() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.flush_subscriptions();
    h.message("/v5/public/linear", SNAPSHOT);
    assert_eq!(
        wire::book_top(&snapshot_books(&mut h)[0]).unwrap().1,
        30247.2
    );
    h.engine.take_frame(h.now);
    h.message("/v5/public/linear", r#"{"topic":"orderbook.1000.BTCUSDT","type":"delta","data":{"s":"BTCUSDT","b":[],"a":[],"u":177400509}}"#);
    h.engine.take_frame(h.now); // Discard any pending incremental frame.
    assert!(h.engine.book("BTCUSDT").is_none());
    let books = snapshot_books(&mut h);
    assert_eq!(books.len(), 1, "resync identity must survive snapshots");
    assert_eq!(&books[0][4..12], &[0; 8]);
    h.message("/v5/public/linear", SNAPSHOT); // Late old subscription cannot recover it.
    assert!(h.engine.book("BTCUSDT").is_none());
    h.advance(100.0);
    h.message("/v5/public/linear", &SNAPSHOT.replace("177400506", "1"));
    assert_eq!(
        wire::book_top(&snapshot_books(&mut h)[0]).unwrap().1,
        30247.2
    );
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    assert_eq!(snapshot_books(&mut h).len(), 1);
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    assert!(snapshot_books(&mut h).is_empty());
}

#[test]
fn resync_enumerates_healthy_and_gapped_categories_together() {
    let mut h = Harness::started(Options {
        enable_inverse_markets: true,
        public_data_only: true,
        ..Options::default()
    });
    let empty = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":""}}"#;
    h.respond("status=PreLaunch", 200, empty);
    h.respond("status=PreLaunch", 200, empty);
    for (category, inverse) in [("linear", false), ("inverse", true)] {
        for (path, rows) in [
            ("instruments-info", support::INSTRUMENTS),
            ("tickers", support::TICKERS),
        ] {
            let index = h
                .requests
                .iter()
                .position(|r| {
                    r.url.contains(path) && r.url.contains(&format!("category={category}"))
                })
                .unwrap();
            let request = h.requests.remove(index);
            let rows = if inverse {
                rows.replace("BTCUSDT", "BTCUSD")
                    .replace("ETHUSDT", "ETHUSD")
                    .replace("LinearPerpetual", "InversePerpetual")
            } else {
                rows.into()
            };
            h.reply(request.id, 200, &rows);
        }
    }
    for (path, symbol) in [
        ("/v5/public/linear", "BTCUSDT"),
        ("/v5/public/inverse", "BTCUSD"),
    ] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
        h.command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        });
        h.flush_subscriptions();
        h.message(path, &SNAPSHOT.replace("BTCUSDT", symbol));
    }
    assert_eq!(snapshot_books(&mut h).len(), 2);
    h.message("/v5/public/inverse", r#"{"topic":"orderbook.1000.BTCUSD","type":"delta","data":{"s":"BTCUSD","b":[],"a":[],"u":177400509}}"#);
    assert!(h.engine.book("BTCUSD").is_none());
    assert!(h.engine.book("BTCUSDT").is_some());
    let books = snapshot_books(&mut h);
    assert_eq!(books.len(), 2);
    assert_eq!(
        books.iter().filter(|b| wire::book_top(b).is_some()).count(),
        1
    );
    assert_eq!(books.iter().filter(|b| b[4..12] == [0; 8]).count(), 1);
}

/// A catalog larger than the engine's interest bound keeps the public feed
/// live: requested books always fit, tickers are capped and the cap is reported.
#[test]
fn oversized_ticker_catalog_caps_tickers_and_keeps_requested_books() {
    use safe_cex_engine::socket_lifecycle::MAX_INTERESTS;
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    let markets = MAX_INTERESTS + 808;
    let rows: Vec<String> = (0..markets)
        .map(|i| {
            format!(
                r#"{{"symbol":"M{i}USDT","contractType":"LinearPerpetual","status":"Trading","baseCoin":"M{i}","quoteCoin":"USDT","deliveryTime":"0","leverageFilter":{{"minLeverage":"1","maxLeverage":"100.00","leverageStep":"0.01"}},"priceFilter":{{"minPrice":"0.01","maxPrice":"199999.98","tickSize":"0.01"}},"lotSizeFilter":{{"maxOrderQty":"7240.00","minOrderQty":"0.01","qtyStep":"0.01"}},"settleCoin":"USDT"}}"#
            )
        })
        .collect();
    let instruments = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"category":"linear","list":[{}],"nextPageCursor":""}},"retExtInfo":{{}},"time":1735809771618}}"#,
        rows.join(",")
    );
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, &instruments);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    let socket = h.socket("/v5/public/linear").expect("public socket");
    h.event(Event::WsOpened { socket });
    h.command(Command::SubscribeOrderBook {
        symbol: "M0USDT".into(),
    });
    // About 820 frames at the 10 ms pace; well inside the idle timeout.
    h.advance(10_000.0);
    assert!(
        !h.closed.contains(&socket),
        "the public feed stays live: {:?}",
        h.logs
    );
    let topics: Vec<String> = h
        .sent_on("/v5/public/linear")
        .iter()
        .filter(|text| text.contains(r#""op":"subscribe""#))
        .flat_map(|text| {
            let frame: serde_json::Value = serde_json::from_str(text).unwrap();
            frame["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(topics.iter().any(|t| t == "orderbook.1000.M0USDT"));
    assert_eq!(topics.len(), MAX_INTERESTS);
    assert_eq!(
        topics.iter().filter(|t| t.starts_with("tickers.")).count(),
        MAX_INTERESTS - 1
    );
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.contains("exceed the socket interest limit"))
            .count(),
        1
    );
}
