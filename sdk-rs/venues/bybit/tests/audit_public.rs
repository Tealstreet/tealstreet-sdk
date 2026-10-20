//! Snapshot recovery after a documented V5 orderbook sequence gap.
mod support;

use safe_cex_engine::{Command, Event, wire};
use safe_cex_venue_bybit::Options;
use support::Harness;

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
