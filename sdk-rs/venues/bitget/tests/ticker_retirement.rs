//! Synthetic REST catalogue/socket fixtures; no exchange account or network calls.
mod support;
use safe_cex_engine::{Command, Event, Level, model::Interval};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::{CONTRACTS, FixtureOptions, Harness, TICKERS};

fn missing(product: &str, symbol: &str, precision: bool) -> String {
    let suffix = if precision { ",precision:null" } else { "" };
    json!({"event":"error", "arg":{"instType":product,"channel":"ticker","instId":symbol},
        "code":"30001", "msg":format!("instType:{product},channel:ticker,instId:{symbol}{suffix} doesn't exist")}).to_string()
}

fn subscribed(h: &Harness, channel: &str, symbol: &str) -> usize {
    h.sent
        .iter()
        .filter_map(|(_, text)| serde_json::from_str::<Value>(text).ok())
        .filter(|wire| wire["op"] == "subscribe")
        .flat_map(|wire| wire["args"].as_array().cloned().unwrap_or_default())
        .filter(|arg| arg["channel"] == channel && arg["instId"] == symbol)
        .count()
}

#[test]
fn exact_precision_null_refusal_is_a_once_only_non_error_notice() {
    let mut h = Harness::public();
    for _ in 0..3 {
        h.message(&missing("USDT-FUTURES", "BTCUSDT", true));
    }
    let notices: Vec<_> = h
        .events
        .iter()
        .filter(|e| e.code == "subscription.rejected")
        .collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].level, Level::Warn);
    assert_eq!(
        notices[0].message,
        "Bitget ticker subscription refused for a known market"
    );
}

#[test]
fn proven_missing_ticker_is_not_retried_or_resent_on_any_public_socket() {
    for precision in [false, true] {
        let mut h = Harness::public();
        let before = subscribed(&h, "ticker", "BTCUSDT");
        h.message(&missing("USDT-FUTURES", "BTCUSDT", precision));
        assert!(
            !h.timers.iter().any(|(_, at)| *at == h.now + 1000.0),
            "permanent ticker refusal must not schedule a retry"
        );
        for (socket, _) in h.sockets.clone() {
            h.event(Event::WsOpened { socket });
        }
        assert_eq!(subscribed(&h, "ticker", "BTCUSDT"), before);
        assert!(
            h.engine.store().market("BTCUSDT").is_some(),
            "WS availability never deletes the REST market"
        );
    }
}

#[test]
fn permanent_refusal_cancels_an_already_pending_transient_retry() {
    let mut h = Harness::public();
    let transient = missing("USDT-FUTURES", "BTCUSDT", false).replace("30001", "30002");
    h.message(&transient);
    let timer = h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 1000.0)
        .unwrap()
        .0;
    h.message(&missing("USDT-FUTURES", "BTCUSDT", true));
    assert!(h.timers.iter().all(|(id, _)| *id != timer));
    let before = subscribed(&h, "ticker", "BTCUSDT");
    h.event(Event::TimerFired { timer });
    assert_eq!(
        subscribed(&h, "ticker", "BTCUSDT"),
        before,
        "a retired callback cannot resend it"
    );
}

#[test]
fn retirement_does_not_suppress_books_candles_or_other_ticker_identities() {
    let mut h = Harness::started(FixtureOptions {
        public_data_only: true,
        ..Default::default()
    });
    let mut catalogue: Value = serde_json::from_str(CONTRACTS).unwrap();
    let mut eth = catalogue["data"][0].clone();
    eth["symbol"] = "ETHUSDT".into();
    eth["baseCoin"] = "ETH".into();
    catalogue["data"].as_array_mut().unwrap().push(eth);
    h.respond("/contracts", 200, &catalogue.to_string());
    h.respond("/tickers", 200, TICKERS);
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    h.message(&missing("USDT-FUTURES", "BTCUSDT", true));
    let btc = subscribed(&h, "ticker", "BTCUSDT");
    let eth = subscribed(&h, "ticker", "ETHUSDT");
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    });
    assert!(subscribed(&h, "books", "BTCUSDT") > 0);
    assert!(subscribed(&h, "candle1m", "BTCUSDT") > 0);
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    assert_eq!(subscribed(&h, "ticker", "BTCUSDT"), btc);
    assert!(subscribed(&h, "ticker", "ETHUSDT") > eth);
    assert_eq!(h.engine.store().markets().len(), 2);
}

#[test]
fn foreign_or_noncanonical_refusal_does_not_retire_the_catalogue_topic() {
    let exact = missing("USDT-FUTURES", "BTCUSDT", true);
    let controls = [
        exact.replace("30001", "30002"),
        exact.replace("precision:null", "precision:0"),
        exact.replace("precision:null", "precision:null,foreign:true"),
        exact.replace("instId:BTCUSDT,precision", "instId:ETHUSDT,precision"),
        exact.replace("USDT-FUTURES", "COIN-FUTURES"),
        exact.replace("ticker", "books"),
    ];
    for refusal in controls {
        let mut h = Harness::public();
        let before = subscribed(&h, "ticker", "BTCUSDT");
        h.message(&refusal);
        assert!(
            h.events
                .iter()
                .any(|e| e.code == "subscription.rejected" && e.level == Level::Error)
        );
        for (socket, _) in h.sockets.clone() {
            h.event(Event::WsOpened { socket });
        }
        assert!(subscribed(&h, "ticker", "BTCUSDT") > before, "{refusal}");
    }
}

#[test]
fn refusal_retirement_is_bound_to_each_exact_product_variant() {
    for (product, testnet, inst, quote) in [
        (Product::Usdt, false, "USDT-FUTURES", "USDT"),
        (Product::Usdt, true, "SUSDT-FUTURES", "SUSDT"),
        (Product::Usdc, false, "USDC-FUTURES", "USDC"),
        (Product::Usdc, true, "SUSDC-FUTURES", "SUSDC"),
        (Product::Inverse, false, "COIN-FUTURES", "USD"),
        (Product::Inverse, true, "SCOIN-FUTURES", "USD"),
    ] {
        let mut h = Harness::started(FixtureOptions {
            public_data_only: true,
            product,
            testnet,
            ..Default::default()
        });
        let symbol = format!("ANTHROPIC{quote}");
        let mut catalogue: Value = serde_json::from_str(CONTRACTS).unwrap();
        catalogue["data"][0]["symbol"] = symbol.clone().into();
        catalogue["data"][0]["baseCoin"] = "ANTHROPIC".into();
        catalogue["data"][0]["quoteCoin"] = quote.into();
        h.respond("/contracts", 200, &catalogue.to_string());
        h.respond("/tickers", 200, &TICKERS.replace("BTCUSDT", &symbol));
        for (socket, _) in h.sockets.clone() {
            h.event(Event::WsOpened { socket });
        }
        let before = subscribed(&h, "ticker", &symbol);
        assert!(before > 0);
        h.message(&missing(inst, &symbol, true));
        h.event(Event::WsOpened {
            socket: h.sockets[0].0,
        });
        assert_eq!(subscribed(&h, "ticker", &symbol), before, "{inst}");
    }
}

#[test]
fn fresh_engine_does_not_inherit_another_engines_retired_topic() {
    let mut old = Harness::public();
    old.message(&missing("USDT-FUTURES", "BTCUSDT", true));
    let fresh = Harness::public();
    assert!(subscribed(&fresh, "ticker", "BTCUSDT") > 0);
    assert!(
        fresh
            .events
            .iter()
            .all(|e| e.code != "subscription.rejected")
    );
}
