//! The concrete binding must expose typed market refusals with their original cause.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(value) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return value;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted markets did not settle");
}

#[test]
fn typed_market_refusal_reports_the_base_and_omits_the_row() {
    for empty_cache in [false, true] {
        let (venue, rt, store, wire) = support::fixture();
        if empty_cache {
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    markets: Some(vec![]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let cached = store.markets().to_vec();
        wire.bodies.lock().unwrap().insert(
            "/api/v1/futures/market/trading_pairs".into(),
            json!({"code":0,"data":[{"base":false,"symbol":"BTCUSDT","quote":"USDT","symbolStatus":"OPEN","isApiSupported":true}]}),
        );
        let markets = drive(&rt, venue.fetch_markets()).unwrap();
        assert!(
            markets.is_empty(),
            "all refused rows produce an empty fresh result"
        );
        assert_eq!(
            store.markets().as_ref(),
            cached.as_slice(),
            "fetch does not mutate the Store"
        );
        let logs = rt.console.borrow();
        assert!(
            logs.iter().any(|(severity, cause)| severity == "warning"
                && cause.contains("Refused Bitunix market row")
                && cause.contains("base=false")),
            "typed market refusal must visibly report its original base value: {logs:?}"
        );
        drop(logs);
        venue.dispose().unwrap();
    }
}

fn instrument(symbol: &str, base: &str) -> serde_json::Value {
    json!({"symbol":symbol,"base":base,"quote":"USDT","symbolStatus":"OPEN",
        "isApiSupported":true,"basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":100})
}

#[test]
fn mixed_market_rows_keep_valid_siblings_and_report_each_base_refusal() {
    for value in [
        Some(json!(false)),
        Some(json!({"asset":"wrong"})),
        Some(Value::Null),
        Some(json!(17)),
        None,
    ] {
        let (venue, rt, _, wire) = support::fixture();
        let mut bad = instrument("BADUSDT", "BAD");
        if let Some(value) = &value {
            bad["base"] = value.clone();
        } else {
            bad.as_object_mut().unwrap().remove("base");
        }
        wire.bodies.lock().unwrap().insert(
            "/api/v1/futures/market/trading_pairs".into(),
            json!({"code":0,"data":[instrument("BTCUSDT","BTC"),bad,instrument("ETHUSDT","ETH")]}),
        );
        let markets = drive(&rt, venue.fetch_markets()).unwrap();
        assert_eq!(
            markets
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            ["BTCUSDT", "ETHUSDT"],
            "one refused base must not replace fresh siblings with cached markets"
        );
        let logs = rt.console.borrow();
        let report = logs
            .iter()
            .find(|(_, line)| line.contains("Refused Bitunix market row"))
            .expect("the row refusal must be visible");
        assert_eq!(report.0, "warning");
        assert!(
            report.1.contains("BADUSDT") && report.1.contains("base="),
            "{report:?}"
        );
        let expected = value
            .as_ref()
            .map_or_else(|| "undefined".into(), serde_json::Value::to_string);
        assert!(report.1.contains(&expected), "{report:?}");
        drop(logs);
        venue.dispose().unwrap();
    }
}

#[test]
fn market_refresh_commits_valid_rows_around_a_refused_base() {
    let (venue, rt, store, wire) = support::fixture();
    let mut bad = instrument("BADUSDT", "BAD");
    bad["base"] = json!(false);
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/market/trading_pairs".into(),
        json!({"code":0,"data":[instrument("BTCUSDT","BTC"),bad,instrument("ETHUSDT","ETH")]}),
    );
    drive(&rt, venue.refresh_markets()).unwrap();
    assert_eq!(
        store
            .markets()
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "ETHUSDT"]
    );
    assert!(
        rt.console
            .borrow()
            .iter()
            .any(|(_, line)| line.contains("Refused Bitunix market row"))
    );
    venue.dispose().unwrap();
}

#[test]
fn refused_base_does_not_coerce_an_unused_maximum() {
    let (venue, rt, _, wire) = support::fixture();
    let mut bad = instrument("BADUSDT", "BAD");
    bad["base"] = json!(false);
    bad["maxLimitOrderVolume"] = json!({"toString":null});
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/market/trading_pairs".into(),
        json!({"code":0,"data":[instrument("BTCUSDT","BTC"),bad,instrument("ETHUSDT","ETH")]}),
    );
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(
        markets
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "ETHUSDT"]
    );
    assert!(
        rt.console
            .borrow()
            .iter()
            .any(|(severity, line)| severity == "warning" && line.contains("base=false"))
    );
    venue.dispose().unwrap();
}
