#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_grvt::data_reads::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/data-reads-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "DATA source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("DATA unexpectedly pending"),
    }
}
fn market() -> Market {
    serde_json::from_value(json!({"id":"btc_perp","symbol":"BTC_USDT_PERP","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.2},"limits":{"amount":{"min":0.01,"max":999},"leverage":{"min":1,"max":25}},"info":{"ih":"0x11","bd":3,"qd":6},"isPerpetual":true})).unwrap()
}
fn position(id: &str) -> Position {
    serde_json::from_value(json!({"id":id,"symbol":"BTC_USDT_PERP","side":"long","entryPrice":7,"notional":8,"leverage":7,"unrealizedPnl":0,"contracts":2,"liquidationPrice":1})).unwrap()
}
fn store() -> DefaultStore {
    let s = DefaultStore::new();
    let old: FetchedLeverage =
        serde_json::from_value(json!({"buyLeverage":7,"sellLeverage":8})).unwrap();
    Store::update(
        &s,
        StoreUpdate {
            markets: Some(vec![market()]),
            positions: Some(vec![position("p1"), position("p2")]),
            fetched_leverage: Some(IndexMap::from([("OLD".into(), old)])),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    s
}
fn scalar(actual: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(actual.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            format!("{:016x}", actual.to_bits()),
            source["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn error(result: Option<&ExchangeError>, source: &Value, label: &str) {
    match result {
        None => assert!(source.is_null(), "{label} unexpected success"),
        Some(actual) => {
            assert_eq!(
                actual.name,
                source["name"].as_str().unwrap(),
                "{label} native name"
            );
            assert_eq!(
                actual.message,
                source["message"].as_str().unwrap(),
                "{label} native message"
            );
        }
    }
}
fn log<'a>(
    name: &'a str,
    effects: &'a RefCell<Vec<Value>>,
) -> impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a {
    let count = Cell::new(0);
    move |text, severity| {
        count.set(count.get() + 1);
        effects
            .borrow_mut()
            .push(json!(["log", text, severity.as_str()]));
        if name == "first-log-error" && count.get() == 1
            || name == "success-log-error" && severity == LogSeverity::Info
            || name == "catch-log-error" && severity == LogSeverity::Error
        {
            Err(ExchangeError::new("log-fail"))
        } else {
            Ok(())
        }
    }
}
const PUBLIC: [&str; 17] = [
    "missing-market",
    "inherited",
    "missing-data",
    "empty",
    "full",
    "lite",
    "numeric",
    "defaults",
    "non-array",
    "null-row",
    "request-error",
    "first-log-error",
    "success-log-error",
    "catch-log-error",
    "zero-options",
    "nonfinite-options",
    "nan",
];
const LEVER: [&str; 19] = [
    "no-client",
    "no-account",
    "missing-market",
    "inherited",
    "falsy-match",
    "missing-data",
    "empty",
    "full",
    "lite",
    "priority-empty",
    "unmatched",
    "non-array",
    "null-row",
    "null-response",
    "endpoint-error",
    "sdk-error",
    "first-log-error",
    "cache-error",
    "nan",
];

#[test]
fn ohlcv_native_requests_and_candles() {
    let rows = corpus()["candleRows"].as_array().unwrap();
    assert_eq!(rows.len(), 17, "CANDLE independent floor");
    let counts: [Option<usize>; 17] = [
        Some(0),
        Some(2),
        Some(0),
        Some(0),
        Some(2),
        Some(1),
        Some(1),
        Some(1),
        None,
        None,
        None,
        None,
        None,
        None,
        Some(2),
        Some(2),
        Some(1),
    ];
    let mut nonfinite_request = None;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], PUBLIC[index], "CANDLE input order");
        let name = PUBLIC[index];
        let s = store();
        let effects = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let opts = OHLCVOptions {
            symbol: row["symbol"].as_str().unwrap().into(),
            interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::M5),
            from: Some(if index == 14 {
                0.0
            } else if index == 15 {
                f64::INFINITY
            } else {
                11.0
            }),
            to: Some(if index == 14 {
                -0.0
            } else if index == 15 {
                f64::NEG_INFINITY
            } else {
                23.0
            }),
            limit: Some(if index == 14 {
                0.0
            } else if index == 15 {
                f64::INFINITY
            } else {
                17.0
            }),
        };
        let future = fetch_ohlcv(
            &s,
            &opts,
            |path, body| {
                requests.borrow_mut().push((path.to_owned(), body));
                effects.borrow_mut().push(json!(["post", path]));
                std::future::ready(if name == "request-error" {
                    Err(ExchangeError::new("post-fail"))
                } else {
                    Ok(
                        safe_cex_transport::how::json_parse(row["raw"].as_str().unwrap())
                            .into_value()
                            .unwrap(),
                    )
                })
            },
            log(name, &effects),
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "CANDLE eager effects"
        );
        let result = run(future);
        let expected_calls = usize::from(index != 0 && index != 11);
        assert_eq!(
            requests.borrow().len(),
            expected_calls,
            "CANDLE independent request count"
        );
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            expected_calls,
            "CANDLE independent source request count"
        );
        for ((path, body), source) in requests
            .borrow()
            .iter()
            .zip(row["requests"].as_array().unwrap())
        {
            assert_eq!(path, "/lite/v1/kline", "CANDLE native endpoint");
            assert_eq!(
                body.instrument_id.as_deref(),
                if index == 1 { None } else { Some("btc_perp") },
                "CANDLE native instrument"
            );
            assert_eq!(
                body.interval,
                safe_cex_venue_grvt::types::GrvtCandlestickInterval::Ci5M,
                "CANDLE native interval"
            );
            assert_eq!(body.kind, "TRADE", "CANDLE native kind");
            match &body.instrument_id {
                Some(id) => assert_eq!(
                    id,
                    source["body"]["i"].as_str().unwrap(),
                    "CANDLE native source instrument"
                ),
                None => assert_eq!(
                    source["body"]["i"]["undefined"], true,
                    "CANDLE native undefined instrument"
                ),
            }
            assert_eq!(source["body"]["i1"], "CI_5_M", "CANDLE source interval");
            assert_eq!(source["body"]["t"], body.kind, "CANDLE source kind");
            assert_eq!(
                body.start.as_deref(),
                source["body"].get("st").and_then(Value::as_str),
                "CANDLE native start"
            );
            assert_eq!(
                body.end.as_deref(),
                source["body"].get("et").and_then(Value::as_str),
                "CANDLE native end"
            );
            match body.limit {
                Some(limit) => scalar(limit, &source["body"]["l"], "CANDLE native limit"),
                None => assert!(
                    source["body"].get("l").is_none(),
                    "CANDLE native absent limit"
                ),
            }
        }
        match &result {
            Ok(candles) => {
                let n = counts[index].expect("CANDLE unexpected success");
                assert_eq!(candles.len(), n, "CANDLE independent native count");
                let expected = row["result"].as_array().unwrap();
                assert_eq!(expected.len(), n, "CANDLE independent source count");
                for (candle, source) in candles.iter().zip(expected) {
                    for (value, key) in [
                        (candle.timestamp, "timestamp"),
                        (candle.open, "open"),
                        (candle.high, "high"),
                        (candle.low, "low"),
                        (candle.close, "close"),
                        (candle.volume, "volume"),
                    ] {
                        scalar(value, &source[key], &format!("CANDLE native {key}"));
                    }
                    assert!(candle.last_side.is_none(), "CANDLE native absent side");
                }
            }
            Err(_) => assert!(counts[index].is_none(), "CANDLE unexpected rejection"),
        };
        error(result.as_ref().err(), &row["error"], "CANDLE");
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "CANDLE native effect order"
        );
        if index == 15 {
            assert!(
                requests.borrow()[0].1.limit.unwrap().is_infinite(),
                "CANDLE native nonfinite before wire"
            );
            nonfinite_request = Some(requests.borrow()[0].1.clone());
        }
    }
    assert!(
        serde_json::to_value(nonfinite_request.unwrap()).unwrap()["l"].is_null(),
        "CANDLE wire nonfinite policy"
    );
}
#[test]
fn book_native_levels_and_totals() {
    let rows = corpus()["bookRows"].as_array().unwrap();
    assert_eq!(rows.len(), 17, "BOOK independent floor");
    let counts: [Option<(usize, usize)>; 17] = [
        Some((0, 0)),
        Some((3, 2)),
        Some((0, 0)),
        Some((0, 0)),
        Some((3, 2)),
        Some((1, 0)),
        Some((1, 0)),
        Some((2, 0)),
        None,
        None,
        None,
        None,
        None,
        None,
        Some((3, 2)),
        Some((3, 2)),
        Some((1, 1)),
    ];
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], PUBLIC[index], "BOOK input order");
        let name = PUBLIC[index];
        let s = store();
        let effects = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let future = fetch_order_book(
            &s,
            row["symbol"].as_str().unwrap(),
            |path, body| {
                requests.borrow_mut().push((path.to_owned(), body));
                effects.borrow_mut().push(json!(["post", path]));
                std::future::ready(if name == "request-error" {
                    Err(ExchangeError::new("post-fail"))
                } else {
                    Ok(
                        safe_cex_transport::how::json_parse(row["raw"].as_str().unwrap())
                            .into_value()
                            .unwrap(),
                    )
                })
            },
            log(name, &effects),
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "BOOK eager effects"
        );
        let result = run(future);
        assert_eq!(
            requests.borrow().len(),
            usize::from(index != 0 && index != 11),
            "BOOK independent request count"
        );
        let source_requests = row["requests"].as_array().unwrap();
        assert_eq!(
            source_requests.len(),
            usize::from(index != 0 && index != 11),
            "BOOK independent source request count"
        );
        for ((path, body), source) in requests.borrow().iter().zip(source_requests) {
            assert_eq!(path, "/lite/v1/book", "BOOK native endpoint");
            assert_eq!(
                body.instrument_id.as_deref(),
                if index == 1 { None } else { Some("btc_perp") },
                "BOOK native instrument"
            );
            match &body.instrument_id {
                Some(id) => assert_eq!(
                    id,
                    source["body"]["instrument_id"].as_str().unwrap(),
                    "BOOK native source instrument"
                ),
                None => assert_eq!(
                    source["body"]["instrument_id"]["undefined"], true,
                    "BOOK native undefined instrument"
                ),
            }
        }
        match &result {
            Ok(book) => {
                let (bids, asks) = counts[index].expect("BOOK unexpected success");
                assert_eq!(
                    (book.bids.len(), book.asks.len()),
                    (bids, asks),
                    "BOOK independent native side counts"
                );
                for (levels, key, n) in [(&book.bids, "bids", bids), (&book.asks, "asks", asks)] {
                    let expected = row["result"][key].as_array().unwrap();
                    assert_eq!(expected.len(), n, "BOOK independent source side count");
                    for (level, source) in levels.iter().zip(expected) {
                        scalar(level.price, &source["price"], "BOOK native price");
                        scalar(level.amount, &source["amount"], "BOOK native amount");
                        scalar(level.total, &source["total"], "BOOK native total");
                    }
                }
            }
            Err(_) => assert!(counts[index].is_none(), "BOOK unexpected rejection"),
        };
        error(result.as_ref().err(), &row["error"], "BOOK");
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "BOOK native effect order"
        );
    }
}

fn native_cache(actual: &IndexMap<String, FetchedLeverage>, source: &Value, label: &str) {
    for (key, value) in actual {
        let expected = source
            .get(key)
            .unwrap_or_else(|| panic!("{label} missing source key {key}"));
        scalar(
            value.buy_leverage,
            &expected["buyLeverage"],
            &format!("{label} native buy"),
        );
        scalar(
            value.sell_leverage,
            &expected["sellLeverage"],
            &format!("{label} native sell"),
        );
        assert!(
            value.is_fetching.is_none()
                && value.margin_type.is_none()
                && value.position_mode.is_none(),
            "{label} native optional absence"
        );
    }
}
fn sdk<'a>(
    name: &'a str,
    row: &'a Value,
    effects: &'a RefCell<Vec<Value>>,
    payloads: &'a RefCell<Vec<(Value, Value)>>,
) -> impl FnOnce(Value, Value) -> std::future::Ready<Result<Value, ExchangeError>> + 'a {
    move |payload, endpoint| {
        payloads.borrow_mut().push((payload, endpoint));
        effects.borrow_mut().push(json!(["sdk"]));
        std::future::ready(if name == "sdk-error" {
            Err(ExchangeError::new("sdk-fail"))
        } else {
            Ok(
                safe_cex_transport::how::json_parse(row["raw"].as_str().unwrap())
                    .into_value()
                    .unwrap(),
            )
        })
    }
}
fn endpoint<'a>(
    name: &'a str,
    effects: &'a RefCell<Vec<Value>>,
) -> impl FnOnce() -> std::future::Ready<Result<Value, ExchangeError>> + 'a {
    move || {
        effects.borrow_mut().push(json!(["endpoint"]));
        std::future::ready(if name == "endpoint-error" {
            Err(ExchangeError::new("endpoint-fail"))
        } else {
            Ok(json!({"token":"edge-31"}))
        })
    }
}
fn cache_listener(
    s: &DefaultStore,
    name: &str,
    effects: Rc<RefCell<Vec<Value>>>,
    snapshots: Rc<RefCell<Vec<IndexMap<String, FetchedLeverage>>>>,
) -> Box<dyn Fn()> {
    let calls = Cell::new(0);
    let name = name.to_owned();
    Store::subscribe(
        s,
        Box::new(move |data, _| {
            if snapshots.borrow().last().is_none_or(|old| {
                old.len() != data.fetched_leverage.len()
                    || old.iter().zip(data.fetched_leverage.iter()).any(
                        |((key, old), (new_key, new))| {
                            key != new_key
                                || old.buy_leverage.to_bits() != new.buy_leverage.to_bits()
                                || old.sell_leverage.to_bits() != new.sell_leverage.to_bits()
                        },
                    )
            }) {
                effects.borrow_mut().push(json!(["cache"]));
                snapshots.borrow_mut().push(data.fetched_leverage.clone());
                calls.set(calls.get() + 1);
                if name == "cache-error" && calls.get() == 1 {
                    return Err(ExchangeError::new("cache-fail").into());
                }
            } else {
                effects.borrow_mut().push(json!(["position", "p1"]));
            }
            Ok(())
        }),
    )
}

#[test]
fn limits_native_market_projection_and_fallback() {
    let rows = corpus()["limitRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "LIMITS independent floor");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], LEVER[index], "LIMITS input order");
        let name = LEVER[index];
        let markets = vec![market()];
        let effects = RefCell::new(Vec::new());
        let payloads = RefCell::new(Vec::new());
        let future = update_market_leverage_limits(
            &markets,
            name != "no-client",
            || Some(if name == "no-account" { "" } else { "sub-72" }.into()),
            endpoint(name, &effects),
            sdk(name, row, &effects, &payloads),
            log(name, &effects),
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "LIMITS eager effects"
        );
        let result = run(future);
        error(result.as_ref().err(), &row["error"], "LIMITS");
        if let Ok(updated) = &result {
            assert_eq!(updated.len(), 1, "LIMITS independent native count");
            assert_eq!(
                row["result"].as_array().unwrap().len(),
                1,
                "LIMITS independent source count"
            );
            let actual = &updated[0];
            let source = &row["result"][0];
            assert_eq!(
                actual.id,
                source["id"].as_str().unwrap(),
                "LIMITS native id"
            );
            assert_eq!(
                actual.symbol,
                source["symbol"].as_str().unwrap(),
                "LIMITS native symbol"
            );
            scalar(
                actual.limits.leverage.max,
                &source["limits"]["leverage"]["max"],
                "LIMITS native max",
            );
            assert_eq!(actual.limits.leverage.min, 1.0, "LIMITS native min");
            assert_eq!(actual.info, market().info, "LIMITS native signing metadata");
            assert_eq!(
                matches!(updated, std::borrow::Cow::Borrowed(_)),
                row["sameArray"].as_bool().unwrap(),
                "LIMITS original input fallback"
            );
        }
        let expected_calls = usize::from(!matches!(index, 0 | 1 | 14));
        assert_eq!(
            payloads.borrow().len(),
            expected_calls,
            "LIMITS independent SDK call count"
        );
        let source_calls = row["payloads"].as_array().unwrap();
        assert_eq!(
            source_calls.len(),
            expected_calls,
            "LIMITS independent source SDK call count"
        );
        for ((payload, endpoint), source) in payloads.borrow().iter().zip(source_calls) {
            assert_eq!(
                payload["sub_account_id"].as_str(),
                Some("sub-72"),
                "LIMITS native account"
            );
            assert_eq!(
                endpoint["token"].as_str(),
                Some("edge-31"),
                "LIMITS native endpoint"
            );
            assert_eq!(
                payload["sub_account_id"], source["payload"]["sub_account_id"],
                "LIMITS native source account"
            );
            assert_eq!(
                endpoint["token"], source["endpoint"]["token"],
                "LIMITS native source endpoint"
            );
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "LIMITS native effect order"
        );
    }
}
#[test]
fn fetch_leverage_native_cache_and_fallback() {
    let rows = corpus()["fetchRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "FETCH LEVERAGE independent floor");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], LEVER[index], "FETCH LEVERAGE input order");
        let name = LEVER[index];
        let s = store();
        let effects = Rc::new(RefCell::new(Vec::new()));
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let _cleanup = cache_listener(&s, name, effects.clone(), snapshots.clone());
        let payloads = RefCell::new(Vec::new());
        let future = fetch_leverage_for_symbol(
            &s,
            row["symbol"].as_str().unwrap(),
            name != "no-client",
            || Some(if name == "no-account" { "" } else { "sub-72" }.into()),
            || 10.0,
            endpoint(name, &effects),
            sdk(name, row, &effects, &payloads),
            log(name, &effects),
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "FETCH LEVERAGE eager effects"
        );
        let result = run(future);
        error(result.as_ref().err(), &row["error"], "FETCH LEVERAGE");
        let expected_calls = usize::from(!matches!(index, 0 | 1 | 2 | 14));
        assert_eq!(
            payloads.borrow().len(),
            expected_calls,
            "FETCH LEVERAGE independent SDK call count"
        );
        let source_calls = row["payloads"].as_array().unwrap();
        assert_eq!(
            source_calls.len(),
            expected_calls,
            "FETCH LEVERAGE independent source SDK call count"
        );
        for ((payload, endpoint), source) in payloads.borrow().iter().zip(source_calls) {
            assert_eq!(
                payload["sub_account_id"].as_str(),
                Some("sub-72"),
                "FETCH LEVERAGE native account"
            );
            assert_eq!(
                payload["sub_account_id"], source["payload"]["sub_account_id"],
                "FETCH LEVERAGE native source account"
            );
            assert_eq!(
                endpoint["token"].as_str(),
                Some("edge-31"),
                "FETCH LEVERAGE native endpoint"
            );
            assert_eq!(
                endpoint["token"], source["endpoint"]["token"],
                "FETCH LEVERAGE native source endpoint"
            );
        }
        if let Ok(Some(value)) = &result {
            scalar(
                value.buy_leverage,
                &row["result"]["buyLeverage"],
                "FETCH LEVERAGE native buy",
            );
            scalar(
                value.sell_leverage,
                &row["result"]["sellLeverage"],
                "FETCH LEVERAGE native sell",
            );
        } else if let Ok(None) = result {
            assert!(
                row["result"].is_null(),
                "FETCH LEVERAGE native missing market"
            );
        }
        let expected_count = if index == 2 { 1 } else { 2 };
        assert_eq!(
            s.fetched_leverage().len(),
            expected_count,
            "FETCH LEVERAGE independent cache count"
        );
        assert_eq!(
            row["cache"].as_object().unwrap().len(),
            expected_count,
            "FETCH LEVERAGE independent source cache count"
        );
        native_cache(&s.fetched_leverage(), &row["cache"], "FETCH LEVERAGE");
        let expected_notifications = if index == 2 {
            0
        } else if index == 17 {
            2
        } else {
            1
        };
        assert_eq!(
            snapshots.borrow().len(),
            expected_notifications,
            "FETCH LEVERAGE independent notification count"
        );
        let source = row["snapshots"].as_array().unwrap();
        assert_eq!(
            source.len(),
            expected_notifications,
            "FETCH LEVERAGE independent source notification count"
        );
        for (actual, source) in snapshots.borrow().iter().zip(source) {
            native_cache(actual, source, "FETCH LEVERAGE notification");
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "FETCH LEVERAGE native effect order"
        );
    }
}
#[test]
fn set_leverage_native_clamp_and_position_order() {
    let rows = corpus()["setRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "SET LEVERAGE independent floor");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], LEVER[index], "SET LEVERAGE input order");
        let name = LEVER[index];
        let s = store();
        let effects = Rc::new(RefCell::new(Vec::new()));
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let _cleanup = cache_listener(&s, name, effects.clone(), snapshots.clone());
        let payloads = RefCell::new(Vec::new());
        let input = if name == "nan" {
            f64::NAN
        } else if name == "lite" {
            -100.0
        } else {
            37.0
        };
        let future = set_leverage(
            &s,
            row["symbol"].as_str().unwrap(),
            input,
            name != "no-client",
            || Some(if name == "no-account" { "" } else { "sub-72" }.into()),
            endpoint(name, &effects),
            sdk(name, row, &effects, &payloads),
            log(name, &effects),
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "SET LEVERAGE eager effects"
        );
        let result = run(future);
        error(result.as_ref().err(), &row["error"], "SET LEVERAGE");
        let expected_calls = usize::from(!matches!(index, 0 | 1 | 2 | 3 | 4 | 14 | 16));
        assert_eq!(
            payloads.borrow().len(),
            expected_calls,
            "SET LEVERAGE independent SDK call count"
        );
        assert_eq!(
            row["payloads"].as_array().unwrap().len(),
            expected_calls,
            "SET LEVERAGE independent source SDK call count"
        );
        for ((payload, endpoint), source) in payloads
            .borrow()
            .iter()
            .zip(row["payloads"].as_array().unwrap())
        {
            assert_eq!(
                payload["sub_account_id"].as_str(),
                Some("sub-72"),
                "SET LEVERAGE native account"
            );
            assert_eq!(
                payload["instrument"].as_str(),
                Some("btc_perp"),
                "SET LEVERAGE native instrument"
            );
            assert_eq!(
                payload["leverage"].as_str(),
                source["payload"]["leverage"].as_str(),
                "SET LEVERAGE native clamp payload"
            );
            assert_eq!(
                endpoint["token"].as_str(),
                Some("edge-31"),
                "SET LEVERAGE native endpoint"
            );
        }
        let changed = matches!(index, 7 | 8 | 17 | 18);
        assert_eq!(
            s.fetched_leverage().len(),
            if changed { 2 } else { 1 },
            "SET LEVERAGE independent cache count"
        );
        native_cache(&s.fetched_leverage(), &row["cache"], "SET LEVERAGE");
        let positions = s.positions();
        assert_eq!(
            positions.len(),
            2,
            "SET LEVERAGE independent position count"
        );
        for (actual, source) in positions.iter().zip(row["positions"].as_array().unwrap()) {
            assert_eq!(
                actual.id,
                source["id"].as_str().unwrap(),
                "SET LEVERAGE native position id"
            );
            scalar(
                actual.leverage,
                &source["leverage"],
                "SET LEVERAGE native position leverage",
            );
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "SET LEVERAGE native cache-before-position order"
        );
    }
}

#[test]
fn declared_owned_market_alias_boundaries() {
    let s = store();
    let original = s.markets().to_vec();
    let result = run(update_market_leverage_limits(
        &original,
        true,
        || Some("sub".into()),
        || std::future::ready(Ok(json!({}))),
        |_, _| std::future::ready(Ok(json!({"r":[{"i":"other","ml1":"9"}]}))),
        |_, _| Ok(()),
    ))
    .unwrap();
    assert!(
        !std::ptr::eq(&result[0], &original[0]),
        "LIMITS declared owned unmatched value"
    );
    assert_eq!(
        corpus()["aliases"]["limitsSameFirst"],
        true,
        "LIMITS measured source retained reference"
    );
    let payload = RefCell::new(None);
    let account = RefCell::new("sub".to_owned());
    let changed_id = Cell::new(false);
    run(set_leverage(
        &s,
        "BTC_USDT_PERP",
        3.0,
        true,
        || Some(account.borrow().clone()),
        || std::future::ready(Ok(json!({}))),
        |body, _| {
            *payload.borrow_mut() = Some(body);
            std::future::ready(Ok(json!({"s":true})))
        },
        |_, _| {
            *account.borrow_mut() = "sub-new".into();
            if !changed_id.replace(true) {
                let mut patch = safe_cex_store::model_patch::ModelPatch::default();
                patch.set_json("id", json!("changed"));
                Store::update_market(&s, "btc_perp", &patch)?;
            }
            Ok(())
        },
    ))
    .unwrap();
    assert_eq!(
        payload.borrow().as_ref().unwrap()["sub_account_id"],
        corpus()["aliases"]["setAccount"],
        "SET LEVERAGE live account after log"
    );
    assert_eq!(
        payload.borrow().as_ref().unwrap()["instrument"],
        "btc_perp",
        "SET LEVERAGE declared captured owned id"
    );
    assert_eq!(
        corpus()["aliases"]["setInstrument"],
        "changed",
        "SET LEVERAGE measured source live id"
    );
    let s = store();
    let result = run(fetch_leverage_for_symbol(
        &s,
        "BTC_USDT_PERP",
        true,
        || Some("sub".into()),
        || 10.0,
        || {
            let mut patch = safe_cex_store::model_patch::ModelPatch::default();
            patch.set_json("id", json!("changed"));
            Store::update_market(&s, "btc_perp", &patch).unwrap();
            std::future::ready(Ok(json!({})))
        },
        |_, _| std::future::ready(Ok(json!({"r":[{"i":"changed","l":"7.5"}]}))),
        |_, _| Ok(()),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        result.buy_leverage, 10.0,
        "FETCH LEVERAGE declared captured owned id"
    );
    scalar(
        7.5,
        &corpus()["aliases"]["fetchValue"]["buyLeverage"],
        "FETCH LEVERAGE measured source live id",
    );
    let s = store();
    let result = run(fetch_leverage_for_symbol(
        &s,
        "BTC_USDT_PERP",
        true,
        || Some("sub".into()),
        || 10.0,
        || std::future::ready(Ok(json!({}))),
        |_, _| std::future::ready(Ok(json!({"r":[{"i":"btc_perp","l":"5.5"}]}))),
        |_, _| Ok(()),
    ))
    .unwrap()
    .unwrap();
    assert!(
        !std::ptr::eq(&result, s.fetched_leverage().get("BTC_USDT_PERP").unwrap()),
        "FETCH LEVERAGE declared returned/cache ownership"
    );
    assert_eq!(
        corpus()["aliases"]["fetchSameCache"],
        true,
        "FETCH LEVERAGE measured source returned/cache alias"
    );
}

#[test]
fn set_reads_position_after_cache_notification() {
    let s = Rc::new(store());
    let weak = Rc::downgrade(&s);
    let changed = Cell::new(false);
    let cleanup = Store::subscribe(
        s.as_ref(),
        Box::new(move |_, _| {
            if !changed.replace(true) {
                let live = weak.upgrade().unwrap();
                Store::update(
                    live.as_ref(),
                    StoreUpdate {
                        positions: Some(vec![position("live")]),
                        ..Default::default()
                    },
                    true,
                )
                .map_err(|error| StoreSubscriberError::new(error.to_string()))?;
            }
            Ok(())
        }),
    );
    run(set_leverage(
        s.as_ref(),
        "BTC_USDT_PERP",
        3.0,
        true,
        || Some("sub".into()),
        || std::future::ready(Ok(json!({}))),
        |_, _| std::future::ready(Ok(json!({"s":true}))),
        |_, _| Ok(()),
    ))
    .unwrap();
    let positions = s.positions();
    assert_eq!(
        positions.len(),
        1,
        "SET LEVERAGE live independent position count"
    );
    assert_eq!(
        positions[0].id, "live",
        "SET LEVERAGE fresh position after cache notification"
    );
    scalar(
        positions[0].leverage,
        &corpus()["aliases"]["postCachePosition"]["leverage"],
        "SET LEVERAGE live native position leverage",
    );
    assert_eq!(
        corpus()["aliases"]["postCachePosition"]["id"],
        positions[0].id,
        "SET LEVERAGE source live position identity"
    );
    cleanup();
}

#[test]
fn set_declared_undefined_account_slot() {
    let s = store();
    let account = RefCell::new(Some("sub".to_owned()));
    let payload = RefCell::new(None);
    run(set_leverage(
        &s,
        "BTC_USDT_PERP",
        3.0,
        true,
        || account.borrow().clone(),
        || std::future::ready(Ok(json!({}))),
        |body, _| {
            *payload.borrow_mut() = Some(body);
            std::future::ready(Ok(json!({"s":true})))
        },
        |_, _| {
            *account.borrow_mut() = None;
            Ok(())
        },
    ))
    .unwrap();
    let body = payload.borrow();
    let body = body.as_ref().unwrap();
    assert!(
        body.get("sub_account_id").is_none(),
        "SET LEVERAGE declared undefined slot omission"
    );
    assert_eq!(
        corpus()["aliases"]["unsetAccount"]["sub_account_id"]["undefined"],
        true,
        "SET LEVERAGE measured source own undefined slot"
    );
    assert_eq!(
        body["instrument"],
        corpus()["aliases"]["unsetAccount"]["instrument"],
        "SET LEVERAGE native unset-slot instrument"
    );
    assert_eq!(
        body["leverage"],
        corpus()["aliases"]["unsetAccount"]["leverage"],
        "SET LEVERAGE native unset-slot leverage"
    );
    assert_eq!(
        serde_json::to_string(body).unwrap(),
        corpus()["aliases"]["unsetWire"].as_str().unwrap(),
        "SET LEVERAGE undefined slot wire projection"
    );
}
