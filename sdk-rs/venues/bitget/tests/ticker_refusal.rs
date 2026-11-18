//! Synthetic public catalogue and socket messages; no account or venue calls.
mod support;
use safe_cex_engine::log::{LogEvent, code};
use safe_cex_engine::{Event, Level};
use support::*;

const TITLE: &str = "Bitget ticker subscription refused for a known market";

fn refusal(channel: &str, symbol: &str, native_code: &str, message: &str) -> String {
    format!(
        r#"{{"event":"error","arg":{{"instType":"USDT-FUTURES","channel":"{channel}","instId":"{symbol}"}},"code":{native_code},"msg":{}}}"#,
        serde_json::to_string(message).unwrap()
    )
}

fn missing(symbol: &str, native_code: &str) -> String {
    refusal(
        "ticker",
        symbol,
        native_code,
        &format!("instType:USDT-FUTURES,channel:ticker,instId:{symbol} doesn't exist"),
    )
}

fn events(h: &Harness) -> Vec<&LogEvent> {
    h.events
        .iter()
        .filter(|event| event.code == code::SUBSCRIPTION_REJECTED)
        .collect()
}

fn subscribes(h: &Harness) -> usize {
    h.sent
        .iter()
        .filter(|(_, text)| {
            text.contains(r#""op":"subscribe""#) && text.contains(r#""channel":"ticker""#)
        })
        .count()
}

// Proven nonexistent tickers now retire (regression 21cc20fce). Unproven
// refusals retain the backoff/recovery coverage below, not futile ticker sends.
#[test]
fn known_ticker_refusal_warns_once_and_retires_its_topic() {
    let mut h = Harness::public();
    let initial = subscribes(&h);
    for _ in 0..8 {
        h.message(&missing("BTCUSDT", "30001"));
    }
    assert_eq!(events(&h).len(), 1);
    assert_eq!(events(&h)[0].level, Level::Warn);
    assert_eq!(events(&h)[0].message, TITLE);
    assert!(!h.timers.iter().any(|(_, at)| *at == h.now + 1000.0));
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    assert_eq!(subscribes(&h), initial);
}

#[test]
fn recovery_still_clears_pending_retry_and_restarts_the_original_backoff() {
    let mut h = Harness::public();
    h.message(&missing("BTCUSDT", "30002"));
    let timer = h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 1000.)
        .unwrap()
        .0;
    h.message(r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","lastPr":"111"}]}"#);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 111.);
    assert!(h.timers.iter().all(|(id, _)| *id != timer));
    let initial = subscribes(&h);
    h.message(&missing("BTCUSDT", "30002"));
    h.timer_after(1000.);
    assert_eq!(subscribes(&h), initial + 1);
    assert_eq!(
        events(&h).len(),
        2,
        "unproven refusals remain visible before and after recovery"
    );
    assert!(events(&h).iter().all(|event| event.level == Level::Error));
}

#[test]
fn catalogue_rename_and_string_native_code_keep_the_same_warning_title() {
    let mut h = Harness::started(FixtureOptions {
        public_data_only: true,
        ..Default::default()
    });
    h.respond("/contracts", 200, &CONTRACTS.replace("BTC", "ANTHROPIC"));
    h.respond("/tickers", 200, &TICKERS.replace("BTC", "ANTHROPIC"));
    let socket = h.sockets[0].0;
    h.event(Event::WsOpened { socket });
    assert!(
        h.sent
            .iter()
            .any(|(_, text)| text.contains("ANTHROPICUSDT"))
    );
    h.message(&refusal(
        "ticker",
        "ANTHROPICUSDT",
        r#""30001""#,
        "ANTHROPICUSDT doesn't exist",
    ));
    assert_eq!(events(&h).len(), 1);
    assert_eq!(events(&h)[0].level, Level::Warn);
    assert_eq!(events(&h)[0].message, TITLE);
}

#[test]
fn unrelated_or_unproven_refusals_remain_errors_on_every_occurrence() {
    let controls = [
        missing("UNKNOWNUSDT", "30001"),
        missing("BTCUSDT", "30002"),
        missing("BTCUSDT", "null"),
        missing("BTCUSDT", "true"),
        missing("BTCUSDT", "30001.000000000000001"),
        missing("BTCUSDT", r#""30001junk""#),
        missing("BTCUSDT", "30001,\"code\":30002"),
        refusal("books", "BTCUSDT", "30001", "BTCUSDT doesn't exist"),
        refusal("candle1m", "BTCUSDT", "30001", "BTCUSDT doesn't exist"),
        refusal("ticker", "BTCUSDT", "30001", "channel doesn't exist"),
        refusal("ticker", "BTCUSDT", "30001", "ETHUSDT doesn't exist"),
        refusal("ticker", "BTCUSDT", "30001", "NOTBTCUSDT doesn't exist"),
        missing("BTCUSDT", "30001").replace("USDT-FUTURES", "COIN-FUTURES"),
    ];
    for text in controls {
        let mut h = Harness::public();
        h.message(&text);
        h.message(&text);
        assert_eq!(events(&h).len(), 2, "{text}");
        assert!(
            events(&h).iter().all(|event| event.level == Level::Error),
            "{text}"
        );
    }
}

#[test]
fn known_topics_warn_independently_and_never_remove_rest_markets() {
    let mut h = Harness::started(FixtureOptions {
        public_data_only: true,
        ..Default::default()
    });
    let mut contracts: serde_json::Value = serde_json::from_str(CONTRACTS).unwrap();
    let mut eth = contracts["data"][0].clone();
    eth["symbol"] = "ETHUSDT".into();
    eth["baseCoin"] = "ETH".into();
    contracts["data"].as_array_mut().unwrap().push(eth);
    h.respond("/contracts", 200, &contracts.to_string());
    h.respond("/tickers", 200, TICKERS);
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    for _ in 0..2 {
        for (index, symbol) in ["BTCUSDT", "ETHUSDT"].iter().enumerate() {
            h.event(Event::WsMessage {
                socket: h.sockets[index].0,
                text: &missing(symbol, "30001"),
            });
        }
    }
    assert_eq!(events(&h).len(), 2);
    assert!(
        events(&h)
            .iter()
            .all(|event| event.level == Level::Warn && event.message == TITLE)
    );
    assert_eq!(h.engine.store().markets().len(), 2);
}
