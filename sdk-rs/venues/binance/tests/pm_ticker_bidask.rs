//! Synthetic venue frames shaped after Binance's public stream documentation.
//! `!bookTicker` is sampled every 5s; `<symbol>@bookTicker` is real time:
//! https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/ws-streams/public
mod support;

use safe_cex_engine::{Command, Event, SocketId};
use safe_cex_venue_binance::Variant;
use serde_json::{Value, json};
use support::{Harness, Options};

fn fixture(name: &str) -> String {
    let frames: Value =
        serde_json::from_str(include_str!("fixtures/pm-ticker-bidask.json")).unwrap();
    frames[name].to_string()
}

fn subscribed(h: &Harness, socket: SocketId, topic: &str) -> bool {
    h.sent.iter().any(|(sent_socket, text)| {
        let frame: Value = serde_json::from_str(text).unwrap();
        *sent_socket == socket
            && frame["method"] == "SUBSCRIBE"
            && frame["params"].as_array().unwrap().contains(&json!(topic))
    })
}

fn seed(h: &mut Harness) {
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
}

fn pm() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::PortfolioMargin,
        public_data_only: true,
        ..Options::default()
    });
    h.load_public();
    h
}

#[test]
fn pm_realtime_quote_matches_depth_after_a_point_one_percent_move() {
    let mut h = pm();
    assert!(h.engine.book("BTCUSDT").is_none());
    let socket = h.socket("/public/ws").unwrap();
    // Model venue delivery: a per-symbol frame arrives only if subscribed.
    // There is no new all-market sample within this 100ms price move.
    h.advance(100.0);
    if subscribed(&h, socket, "btcusdt@bookTicker") {
        h.message("/public/ws", &fixture("bookTicker"));
    }
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.message("/public/ws", &fixture("depthUpdate"));
    h.respond("/fapi/v1/depth", 200, &fixture("depthSnapshot"));
    let book = h.engine.book("BTCUSDT").unwrap();
    assert_eq!(
        (book.best_bid(), book.best_ask()),
        (Some(99.8), Some(100.0))
    );
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));
    assert_eq!(
        (Some(ticker.bid), Some(ticker.ask)),
        (book.best_bid(), book.best_ask())
    );
    assert!(!subscribed(&h, socket, "!bookTicker"));

    h.message("/market/ws", &fixture("daily"));
    h.message("/market/ws", &fixture("premium"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));
    assert_eq!(
        (ticker.last, ticker.mark, ticker.index),
        (100.2, 100.3, 100.4)
    );
}

#[test]
fn pm_reconnect_replays_realtime_quotes_without_a_depth_subscription() {
    let mut h = pm();
    let old = h.socket("/public/ws").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.socket("/public/ws").unwrap();
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    assert!(subscribed(&h, socket, "btcusdt@bookTicker"));
    assert!(!subscribed(&h, socket, "!bookTicker"));
    h.message("/public/ws", &fixture("bookTicker"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));
    assert!(h.engine.book("BTCUSDT").is_none());
}

#[test]
fn pm_catalogue_retry_adds_realtime_quotes_for_new_markets() {
    // Exercise the legacy PM flag as well as the explicit variant above.
    let mut h = Harness::started(Options {
        portfolio_margin: true,
        public_data_only: true,
        ..Options::default()
    });
    let mut info: Value = serde_json::from_str(support::EXCHANGE_INFO).unwrap();
    info["symbols"]
        .as_array_mut()
        .unwrap()
        .push(json!({"symbol":"malformed"}));
    h.respond("/fapi/v1/exchangeInfo", 200, &info.to_string());
    seed(&mut h);
    let socket = h.socket("/public/ws").unwrap();
    h.event(Event::WsOpened { socket });
    h.advance(1000.0);
    let mut eth = info["symbols"][0].clone();
    eth["symbol"] = json!("ETHUSDT");
    eth["baseAsset"] = json!("ETH");
    info["symbols"][1] = eth;
    h.sent.clear();
    h.respond("/fapi/v1/exchangeInfo", 200, &info.to_string());
    assert_eq!(h.engine.store().markets().len(), 2);
    assert!(subscribed(&h, socket, "ethusdt@bookTicker"));
    assert!(
        !subscribed(&h, socket, "btcusdt@bookTicker"),
        "existing interests stay active"
    );
    assert!(!subscribed(&h, socket, "!bookTicker"));

    for (path, body) in [
        ("/fapi/v1/ticker/24hr", support::DAILY),
        ("/fapi/v1/ticker/bookTicker", support::BOOK_TICKERS),
        ("/fapi/v1/premiumIndex", support::PREMIUM),
    ] {
        let mut rows: Vec<Value> = serde_json::from_str(body).unwrap();
        let mut eth = rows[0].clone();
        eth["symbol"] = json!("ETHUSDT");
        rows.push(eth);
        h.respond(path, 200, &serde_json::to_string(&rows).unwrap());
    }
    let mut quote: Value = serde_json::from_str(&fixture("bookTicker")).unwrap();
    quote["s"] = json!("ETHUSDT");
    quote["b"] = json!("149.8");
    quote["a"] = json!("150.0");
    h.message("/public/ws", &quote.to_string());
    let tickers = h.engine.store().tickers();
    let eth = tickers.iter().find(|t| t.symbol == "ETHUSDT").unwrap();
    let btc = tickers.iter().find(|t| t.symbol == "BTCUSDT").unwrap();
    assert_eq!((eth.bid, eth.ask), (149.8, 150.0));
    assert_eq!((btc.bid, btc.ask), (99.9, 100.1));
}
