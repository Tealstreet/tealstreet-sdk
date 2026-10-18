#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{MarginType, Position, PositionMode, PositionSide};
use safe_cex_venue_grvt::snapshots::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("SNAPSHOT fixture unexpectedly pending"),
    }
}
fn corpus() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots.cjs"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "SNAPSHOT source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}
fn bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8000000000000
    } else {
        value.to_bits()
    }
}
fn expected_bits(value: &Value) -> u64 {
    u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap()
}
fn store(raw: &str) -> DefaultStore {
    let mut data: Value = serde_json::from_str(raw).unwrap();
    data["balance"] = json!({"free":31,"used":7,"total":38,"upnl":0});
    let update = StoreUpdate {
        positions: data
            .get("positions")
            .cloned()
            .map(|v| js_from_value(v).unwrap()),
        markets: data
            .get("markets")
            .cloned()
            .map(|v| js_from_value(v).unwrap()),
        tickers: data
            .get("tickers")
            .cloned()
            .map(|v| js_from_value(v).unwrap()),
        fetched_leverage: data
            .get("fetchedLeverage")
            .cloned()
            .map(|v| serde_json::from_value(v).unwrap()),
        balance: Some(serde_json::from_value(data["balance"].clone()).unwrap()),
        ..Default::default()
    };
    let store = DefaultStore::new();
    Store::update(&store, update, false).unwrap();
    store
}
fn assert_position(actual: &Position, expected: &Value, label: &str) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "{label} native id"
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "{label} native symbol"
    );
    assert_eq!(
        actual.side,
        match expected["side"].as_str().unwrap() {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            _ => panic!("source side"),
        },
        "{label} native side"
    );
    for (field, value) in [
        ("entryPrice", actual.entry_price),
        ("notional", actual.notional),
        ("leverage", actual.leverage),
        ("unrealizedPnl", actual.unrealized_pnl),
        ("contracts", actual.contracts),
        ("liquidationPrice", actual.liquidation_price),
    ] {
        assert_eq!(
            bits(value),
            expected_bits(&expected[field]),
            "{label} native {field}"
        );
    }
    assert_eq!(
        actual.position_mode,
        expected.get("positionMode").map(|_| PositionMode::OneWay),
        "{label} native mode"
    );
    assert_eq!(
        actual.margin_type,
        expected.get("marginType").map(|_| MarginType::Isolated),
        "{label} native margin"
    );
    assert!(
        actual.info.is_none()
            && actual.extra.is_none()
            && actual.realized_pnl.is_none()
            && actual.margin_size.is_none()
            && actual.last_update.is_none(),
        "{label} absent optional fields"
    );
}

#[test]
fn nonce_ordered_native_cache() {
    let corpus = corpus();
    let rows = corpus["nonceRows"].as_array().unwrap();
    assert_eq!(rows.len(), 209, "NONCE independent call floor");
    let (mut times, mut ids) = (IndexMap::new(), IndexMap::new());
    for (step, row) in rows.iter().enumerate() {
        let args = row["input"].as_array().unwrap();
        record_nonce(
            &mut times,
            &mut ids,
            args[0].as_str().unwrap(),
            args[1].as_str(),
            args[2].as_f64(),
        );
        let expected_count = if step < 5 { 1 } else { (step - 3).min(200) };
        assert_eq!(
            times.len(),
            expected_count,
            "NONCE independent retention count step {step}"
        );
        let expected = row["times"].as_array().unwrap();
        assert_eq!(
            expected.len(),
            expected_count,
            "NONCE source retention count"
        );
        for ((key, time), entry) in times.iter().zip(expected) {
            assert_eq!(
                key,
                entry[0].as_str().unwrap(),
                "NONCE ordered native key step {step}"
            );
            assert_eq!(
                time.map(bits),
                entry[1].get("bits").map(|_| expected_bits(&entry[1])),
                "NONCE ordered native earliest time step {step}"
            );
        }
        let expected = row["ids"].as_array().unwrap();
        assert_eq!(ids.len(), expected.len(), "NONCE real id count");
        for ((key, id), entry) in ids.iter().zip(expected) {
            assert_eq!(key, entry[0].as_str().unwrap(), "NONCE native real key");
            assert_eq!(id, entry[1].as_str().unwrap(), "NONCE native real id");
        }
    }
    assert_eq!(times.first().unwrap().0, "n3", "NONCE oldest retained key");
    assert_eq!(times.last().unwrap().0, "n202", "NONCE final retained key");
}

#[test]
fn mode_native_variant() {
    assert_eq!(corpus()["mode"], "oneway", "MODE actual source");
    assert_eq!(
        run(get_position_mode()),
        PositionMode::OneWay,
        "MODE native variant"
    );
}

#[test]
fn position_pnl_native_bits() {
    let corpus = corpus();
    let rows = corpus["pnlRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "PNL independent source floor");
    for row in rows {
        let store = store(row["raw"].as_str().unwrap());
        let positions = store.positions().unwrap();
        let expected = row["derived"]
            .get("bits")
            .map(|_| expected_bits(&row["derived"]));
        assert_eq!(
            calculate_position_upnl_from_last_price(&positions[0], &store).map(bits),
            expected,
            "POSITION PNL native bits {row}"
        );
    }
}
#[test]
fn total_pnl_native_bits() {
    let corpus = corpus();
    let rows = corpus["pnlRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "TOTAL PNL independent source floor");
    for row in rows {
        let store = store(row["raw"].as_str().unwrap());
        assert_eq!(
            bits(calculate_upnl_from_positions(
                &store.positions().unwrap(),
                &store
            )),
            expected_bits(&row["total"]),
            "TOTAL PNL native bits {row}"
        );
    }
}

#[test]
fn sync_native_store_order() {
    let corpus = corpus();
    let rows = corpus["pnlRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "SYNC independent source floor");
    for (index, row) in rows.iter().enumerate() {
        let store = store(row["raw"].as_str().unwrap());
        let observations = Rc::new(RefCell::new(Vec::new()));
        let capture = observations.clone();
        let prior = RefCell::new(
            store
                .positions()
                .unwrap()
                .iter()
                .map(|p| bits(p.unrealized_pnl))
                .collect::<Vec<_>>(),
        );
        let cleanup = Store::subscribe(
            &store,
            Box::new(move |data, _| {
                let now = data
                    .positions
                    .iter()
                    .map(|p| bits(p.unrealized_pnl))
                    .collect::<Vec<_>>();
                let label = if now != *prior.borrow() {
                    "positions"
                } else {
                    "balance"
                };
                *prior.borrow_mut() = now;
                capture.borrow_mut().push(label.to_owned());
                Ok(())
            }),
        );
        sync_balance_upnl_from_positions(&store, None).unwrap();
        let expected = row["positions"].as_array().unwrap();
        let count = if index <= 16 { 1 } else { 2 };
        assert_eq!(
            store.positions().unwrap().len(),
            count,
            "SYNC native position count"
        );
        assert_eq!(expected.len(), count, "SYNC source position count");
        for (actual, expected) in store.positions().unwrap().iter().zip(expected) {
            assert_position(actual, expected, "SYNC");
        }
        let balance = store.balance().unwrap();
        for (field, value) in [
            ("free", balance.free),
            ("used", balance.used),
            ("total", balance.total),
            ("upnl", balance.upnl),
        ] {
            assert_eq!(
                bits(value),
                expected_bits(&row["balance"][field]),
                "SYNC native balance {field}"
            );
        }
        let expected = row["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v[0].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            *observations.borrow(),
            expected,
            "SYNC native notification order"
        );
        cleanup();
    }
}

#[test]
fn fake_positions_native_fields() {
    let corpus = corpus();
    let rows = corpus["fakeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "FAKE independent floor");
    for (row_index, row) in rows.iter().enumerate() {
        let store = store(row["raw"].as_str().unwrap());
        let result = get_fake_positions(&store, 10.0);
        if row_index == 3 {
            assert_eq!(
                row["expected"][0]["leverage"],
                json!({"undefined":true}),
                "FAKE source inherited undefined"
            );
            assert_eq!(
                result.unwrap_err().name,
                "RepresentationError",
                "FAKE closed typed output refusal"
            );
            continue;
        }
        let actual = result.unwrap();
        let expected = row["expected"].as_array().unwrap();
        assert_eq!(
            actual.len(),
            [0, 2, 1][row_index],
            "FAKE independent native count"
        );
        assert_eq!(
            expected.len(),
            [0, 2, 1][row_index],
            "FAKE independent source count"
        );
        for (actual, expected) in actual.iter().zip(expected) {
            assert_position(actual, expected, "FAKE");
        }
    }
}

#[test]
fn ticker_native_fields_and_errors() {
    let corpus = corpus();
    let rows = corpus["tickerRows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "TICKER independent floor");
    for (index, row) in rows.iter().enumerate() {
        let store = store(row["raw"].as_str().unwrap());
        let mut effects = Vec::new();
        let fail = row["failAt"].as_u64().unwrap();
        let result = run(fetch_tickers(&store, |message, severity| {
            effects.push(json!([message, severity.as_str()]));
            if effects.len() == 1 && fail > 0 || effects.len() == 2 && fail == 2 {
                Err(ExchangeError::new(format!("log-{}", effects.len())))
            } else {
                Ok(())
            }
        }));
        match result {
            Ok(tickers) => {
                let expected = row["result"].as_array().unwrap();
                let count = if index < 3 { 0 } else { 2 };
                assert_eq!(tickers.len(), count, "TICKER independent native count");
                assert_eq!(expected.len(), count, "TICKER independent source count");
                for (t, e) in tickers.iter().zip(expected) {
                    assert_eq!(t.id, e["id"].as_str().unwrap(), "TICKER native id");
                    assert_eq!(
                        t.symbol,
                        e["symbol"].as_str().unwrap(),
                        "TICKER native symbol"
                    );
                    for (field, value) in [
                        ("bid", t.bid),
                        ("ask", t.ask),
                        ("last", t.last),
                        ("mark", t.mark),
                        ("index", t.index),
                        ("percentage", t.percentage),
                        ("openInterest", t.open_interest),
                        ("fundingRate", t.funding_rate),
                        ("fundingTime", t.funding_time),
                        ("volume", t.volume),
                        ("quoteVolume", t.quote_volume),
                    ] {
                        assert_eq!(
                            bits(value),
                            expected_bits(&e[field]),
                            "TICKER native {field}"
                        );
                    }
                    assert!(t.mid_price.is_none(), "TICKER absent midpoint");
                    assert!(t.timestamp.is_none(), "TICKER absent timestamp");
                    assert_eq!(t.info, Some(json!({}).into()), "TICKER native info");
                }
            }
            Err(error) => {
                assert_eq!(
                    error.message,
                    row["error"]["message"].as_str().unwrap(),
                    "TICKER native rejection"
                );
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "TICKER native error name"
                );
            }
        }
        assert_eq!(
            effects,
            row["effects"].as_array().unwrap().to_vec(),
            "TICKER native log order"
        );
    }
}

#[test]
fn dispose_native_effect_order() {
    let corpus = corpus();
    let rows = corpus["disposeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "DISPOSE independent floor");
    for row in rows {
        let disposed = Cell::new(false);
        let effects = RefCell::new(Vec::new());
        let fail = row["fail"].as_str().unwrap();
        let result = dispose(
            &disposed,
            || {
                effects.borrow_mut().push(json!(["public", disposed.get()]));
                if fail == "public" {
                    Err(ExchangeError::new("public"))
                } else {
                    Ok(())
                }
            },
            || {
                effects
                    .borrow_mut()
                    .push(json!(["private", disposed.get()]));
                if fail == "private" {
                    Err(ExchangeError::new("private"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(disposed.get(), "DISPOSE native disposed state");
        assert_eq!(
            result.err().map(|e| e.message).as_deref(),
            row["error"].as_str(),
            "DISPOSE native rejection"
        );
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "DISPOSE native socket order"
        );
    }
}

#[test]
fn orders_native_filter() {
    let corpus = corpus();
    let rows = corpus["ordersRows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "ORDERS independent floor");
    for row in rows {
        let symbols = row["symbols"].as_array().map(|a| {
            a.iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        });
        let mut calls = Vec::new();
        let order: safe_cex_types::Order = js_from_value(json!({"id":"o1","symbol":"A","status":"open","type":"limit","side":"buy","price":7,"amount":3,"filled":1,"remaining":2,"reduceOnly":false,"lastUpdate":123})).unwrap();
        let expected = vec![order.clone()];
        let result = run(fetch_orders(symbols.as_deref(), |symbol| {
            calls.push(symbol.map(str::to_owned));
            let mut error = ExchangeError::new("orders");
            error.name = "TypeError".into();
            error.context = Some(json!({"identity":"original"}));
            std::future::ready(if row["fail"] == true {
                Err(error)
            } else {
                Ok(vec![order])
            })
        }));
        match result {
            Ok(orders) => {
                assert_eq!(orders.len(), 1, "ORDERS independent result count");
                assert_eq!(orders, expected, "ORDERS native result");
                assert_eq!(
                    orders[0].id,
                    row["result"][0]["id"].as_str().unwrap(),
                    "ORDERS source result identity"
                );
            }
            Err(error) => {
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "ORDERS native error name"
                );
                assert_eq!(
                    error.message,
                    row["error"]["message"].as_str().unwrap(),
                    "ORDERS native error message"
                );
                assert_eq!(
                    error.context,
                    Some(json!({"identity":"original"})),
                    "ORDERS original error context"
                );
            }
        }
        assert_eq!(calls.len(), 1, "ORDERS independent call count");
        assert_eq!(
            calls[0].as_deref(),
            row["effects"][0][0].as_str(),
            "ORDERS native forwarded filter"
        );
    }
}

#[test]
fn sync_subscriber_failure_preserves_error() {
    let corpus = corpus();
    let store = store(corpus["pnlRows"][18]["raw"].as_str().unwrap());
    let mut original = ExchangeError::new("listener");
    original.name = "TypeError".into();
    original.context = Some(json!({"identity":"listener"}));
    let cause = Rc::new(original.clone());
    let cleanup = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            Err(safe_cex_types::StoreSubscriberError::new("listener").with_cause(cause.clone()))
        }),
    );
    let error = sync_balance_upnl_from_positions(&store, None).unwrap_err();
    assert_eq!(error.name, original.name, "SYNC original listener name");
    assert_eq!(
        error.message, original.message,
        "SYNC original listener message"
    );
    assert_eq!(
        error.context, original.context,
        "SYNC original listener context"
    );
    assert_eq!(
        store.balance().unwrap().upnl,
        0.0,
        "SYNC failed position notification prevents balance write"
    );
    assert_eq!(
        store.positions().unwrap()[0].unrealized_pnl,
        4.0,
        "SYNC position mutation precedes failure"
    );
    cleanup();
}

#[test]
fn sync_reads_balance_after_position_notification() {
    let corpus = corpus();
    let store = Rc::new(store(corpus["pnlRows"][18]["raw"].as_str().unwrap()));
    let weak = Rc::downgrade(&store);
    let count = Cell::new(0);
    let cleanup = Store::subscribe(
        &*store,
        Box::new(move |_, _| {
            let n = count.get();
            count.set(n + 1);
            if n == 0 {
                let store = weak.upgrade().unwrap();
                let mut balance = store.balance().unwrap().clone();
                balance.free = -123.0;
                Store::update(
                    &*store,
                    StoreUpdate {
                        balance: Some(balance),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            Ok(())
        }),
    );
    sync_balance_upnl_from_positions(&*store, None).unwrap();
    assert_eq!(
        store.balance().unwrap().free,
        -123.0,
        "SYNC live balance read after notification"
    );
    assert_eq!(
        store.balance().unwrap().upnl,
        2.0,
        "SYNC balance sum after notification"
    );
    cleanup();
}

#[test]
fn account_native_result_and_request() {
    let corpus = corpus();
    let rows = corpus["accountRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "ACCOUNT independent floor");
    for row in rows {
        let effects = RefCell::new(Vec::new());
        let live_account = RefCell::new(row["account"].as_str().map(str::to_owned));
        let result = run(get_account(
            row["sub"].as_str(),
            || live_account.borrow().clone(),
            |url, body| {
                effects.borrow_mut().push(json!(["post", url, body]));
                async {
                    if let Some(account) = row["lateAccount"].as_str() {
                        *live_account.borrow_mut() = Some(account.to_owned());
                    }
                    if row["fail"] == true {
                        Err(ExchangeError::new("request"))
                    } else {
                        Ok(json!({"data":{}}))
                    }
                }
            },
            |message, severity| {
                effects
                    .borrow_mut()
                    .push(json!(["log", message, severity.as_str()]));
                Ok(())
            },
        ));
        match result {
            Ok((user, affiliate)) => {
                assert_eq!(
                    user,
                    row["result"]["userId"].as_str().unwrap(),
                    "ACCOUNT native user id"
                );
                assert!(affiliate.is_none(), "ACCOUNT native affiliate null");
            }
            Err(error) => assert_eq!(
                error.message,
                row["error"].as_str().unwrap(),
                "ACCOUNT native rejection"
            ),
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "ACCOUNT native request and log order"
        );
    }
}
