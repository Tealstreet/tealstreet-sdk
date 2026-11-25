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
    sent_with(h, socket, "SUBSCRIBE", topic)
}

fn sent_with(h: &Harness, socket: SocketId, method: &str, topic: &str) -> bool {
    h.sent.iter().any(|(sent_socket, text)| {
        let frame: Value = serde_json::from_str(text).unwrap();
        *sent_socket == socket
            && frame["method"] == method
            && frame["params"].as_array().unwrap().contains(&json!(topic))
    })
}

fn quote(symbol: &str, update_id: u64, bid: &str, ask: &str) -> String {
    let mut frame: Value = serde_json::from_str(&fixture("bookTicker")).unwrap();
    frame["s"] = json!(symbol);
    frame["u"] = json!(update_id);
    frame["b"] = json!(bid);
    frame["a"] = json!(ask);
    frame.to_string()
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

fn subscribe_book(h: &mut Harness) {
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
}

/// AGT-175: a per-symbol stream for every market (~950 on USD-M) made each quiet
/// market its own stale topic, and the client flagged the account delayed or disconnected.
#[test]
fn pm_quotes_stream_on_one_all_market_topic_without_a_book() {
    let mut h = pm();
    let socket = h.socket("/public/ws").unwrap();
    assert!(subscribed(&h, socket, "!bookTicker"));
    assert!(!subscribed(&h, socket, "btcusdt@bookTicker"));

    h.message("/public/ws", &fixture("bookTicker"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));
    assert!(h.engine.book("BTCUSDT").is_none());

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
fn pm_book_symbol_quote_streams_in_real_time_and_matches_depth() {
    let mut h = pm();
    let socket = h.socket("/public/ws").unwrap();
    subscribe_book(&mut h);
    assert!(subscribed(&h, socket, "btcusdt@depth"));
    assert!(subscribed(&h, socket, "btcusdt@bookTicker"));

    // A 100ms price move with no new all-market sample: only the symbol stream has it.
    h.advance(100.0);
    h.message("/public/ws", &fixture("bookTicker"));
    h.message("/public/ws", &fixture("depthUpdate"));
    h.respond("/fapi/v1/depth", 200, &fixture("depthSnapshot"));
    let book = h.engine.book("BTCUSDT").unwrap();
    assert_eq!(
        (book.best_bid(), book.best_ask()),
        (Some(99.8), Some(100.0))
    );
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!(
        (Some(ticker.bid), Some(ticker.ask)),
        (book.best_bid(), book.best_ask())
    );

    h.sent.clear();
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    assert!(sent_with(&h, socket, "UNSUBSCRIBE", "btcusdt@bookTicker"));
    assert!(!sent_with(&h, socket, "UNSUBSCRIBE", "!bookTicker"));
}

#[test]
fn pm_older_all_market_sample_does_not_overwrite_a_newer_symbol_quote() {
    let mut h = pm();
    subscribe_book(&mut h);
    h.message("/public/ws", &quote("BTCUSDT", 160, "99.8", "100.0"));
    // The 5s all-market sample can arrive after newer real-time updates.
    h.message("/public/ws", &quote("BTCUSDT", 150, "99.5", "99.7"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));

    h.message("/public/ws", &quote("BTCUSDT", 170, "99.9", "100.1"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.9, 100.1));
}

#[test]
fn pm_reconnect_replays_the_all_market_and_book_quote_topics() {
    let mut h = pm();
    subscribe_book(&mut h);
    let old = h.socket("/public/ws").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.socket("/public/ws").unwrap();
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    assert!(subscribed(&h, socket, "!bookTicker"));
    assert!(subscribed(&h, socket, "btcusdt@bookTicker"));
    h.message("/public/ws", &fixture("bookTicker"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.8, 100.0));
}

#[test]
fn pm_reopened_book_resubscribes_its_quote_and_starts_a_fresh_update_id() {
    let mut h = pm();
    let socket = h.socket("/public/ws").unwrap();
    subscribe_book(&mut h);
    h.message("/public/ws", &quote("BTCUSDT", 160, "99.8", "100.0"));
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    assert!(sent_with(&h, socket, "UNSUBSCRIBE", "btcusdt@bookTicker"));

    h.sent.clear();
    subscribe_book(&mut h);
    assert!(subscribed(&h, socket, "btcusdt@bookTicker"));
    h.message("/public/ws", &quote("BTCUSDT", 150, "99.5", "99.7"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.5, 99.7));
}

/// A reopened socket may restart a symbol's update-id sequence; a kept id would
/// freeze the open book's bid and ask.
#[test]
fn pm_reconnect_with_an_open_book_accepts_a_restarted_update_id() {
    let mut h = pm();
    subscribe_book(&mut h);
    h.message("/public/ws", &quote("BTCUSDT", 160, "99.8", "100.0"));
    let old = h.socket("/public/ws").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.socket("/public/ws").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/public/ws", &quote("BTCUSDT", 5, "99.5", "99.7"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.bid, ticker.ask), (99.5, 99.7));

    h.message("/public/ws", &quote("BTCUSDT", 4, "99.1", "99.3"));
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!(
        (ticker.bid, ticker.ask),
        (99.5, 99.7),
        "the guard still applies"
    );
}

#[test]
fn pm_catalogue_retry_adds_markets_without_new_quote_subscriptions() {
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
    assert!(
        !h.sent
            .iter()
            .any(|(s, text)| *s == socket && text.contains("SUBSCRIBE")),
        "the all-market stream already covers new markets"
    );

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
    h.message("/public/ws", &quote("ETHUSDT", 160, "149.8", "150.0"));
    let tickers = h.engine.store().tickers();
    let eth = tickers.iter().find(|t| t.symbol == "ETHUSDT").unwrap();
    let btc = tickers.iter().find(|t| t.symbol == "BTCUSDT").unwrap();
    assert_eq!((eth.bid, eth.ask), (149.8, 150.0));
    assert_eq!((btc.bid, btc.ask), (99.9, 100.1));
}
