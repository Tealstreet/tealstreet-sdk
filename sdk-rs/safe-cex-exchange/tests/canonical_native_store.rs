//! Real canonical owner/facade operations against maintained Store witnesses.
use safe_cex_exchange::{
    js_runtime::{
        globals,
        value::{Js, Thrown, from_json},
    },
    native_store::DefaultStore,
};
use safe_cex_types::StoreReadPhase;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    process::Command,
    rc::{Rc, Weak},
};

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_live_view_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fixture() -> (Rc<DefaultStore>, Js) {
    let native = Rc::new(DefaultStore::new());
    let facade = safe_cex_exchange::js_runtime::store::facade(native.clone()).unwrap();
    facade.call_method("update", vec![from_json(&json!({
        "markets":[{"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
            "precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}}],
        "tickers":[{"id":"t","symbol":"BTCUSD","bid":1,"ask":2,"last":1,"mark":1,"index":1,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0}]
    }))]).unwrap();
    (native, facade)
}

#[test]
fn native_graph_retains_raw_rows_and_all_four_live_dictionary_aliases() {
    let expected = source();
    let (_, facade) = fixture();
    for case in expected["identity"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let market = name.starts_with("markets");
        let key = if name.ends_with("BySymbol") {
            "BTCUSD"
        } else if market {
            "m"
        } else {
            "t"
        };
        let table = facade.get(name).unwrap();
        let item = table.get(key).unwrap();
        let field = if market { "active" } else { "bid" };
        let value = if market {
            Js::Bool(false)
        } else {
            Js::Number(7.0)
        };
        facade
            .call_method(
                if market {
                    "updateMarket"
                } else {
                    "updateTicker"
                },
                vec![
                    Js::object(vec![("id", Js::str(if market { "m" } else { "t" }))]),
                    Js::object(vec![(field, value.clone())]),
                ],
            )
            .unwrap();
        assert!(
            item.get(field).unwrap().strict_equals(&value),
            "retained {name} row must see granular write"
        );
        assert!(table.same_reference(&facade.get(name).unwrap()));
        assert!(item.same_reference(&table.get(key).unwrap()));
        assert!(
            item.same_reference(
                &facade
                    .get(if market { "markets" } else { "tickers" })
                    .unwrap()
                    .get("0")
                    .unwrap()
            )
        );
    }
    let row = Js::object(vec![
        ("id", Js::str("raw")),
        ("symbol", Js::str("raw")),
        ("opaque", Js::utf16(&[0xd800])),
    ]);
    let rows = Js::array(vec![row.clone()]);
    facade
        .call_method("update", vec![Js::object(vec![("markets", rows.clone())])])
        .unwrap();
    assert!(facade.get("markets").unwrap().same_reference(&rows));
    assert!(
        facade
            .get("marketsById")
            .unwrap()
            .get("raw")
            .unwrap()
            .same_reference(&row)
    );
}

#[test]
fn captured_balance_is_lazy_retains_generation_and_does_not_retain_store_owner() {
    assert_eq!(source()["balances"].as_array().unwrap().len(), 2);
    let (native, facade) = fixture();
    let owner = Rc::downgrade(&native);
    let balance = facade.get("balance").unwrap();
    let handle = native.balance_handle().unwrap();
    let native_handle = handle.native_value().downcast::<Js>().unwrap();
    assert!(native_handle.same_reference(&balance));
    balance.set("total", Js::Number(-0.0)).unwrap();
    assert_eq!(handle.read().unwrap().total.to_bits(), (-0.0_f64).to_bits());
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "balance",
                Js::object(vec![
                    ("used", Js::Number(1.0)),
                    ("free", Js::Number(2.0)),
                    ("total", Js::Number(3.0)),
                    ("upnl", Js::Number(4.0)),
                ]),
            )])],
        )
        .unwrap();
    assert!(!facade.get("balance").unwrap().same_reference(&balance));
    let new_handle = native.balance_handle().unwrap();
    assert_eq!(new_handle.read().unwrap().total, 3.0);
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let thrown = cause.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    balance
        .define_accessor(
            "total",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Err(Thrown::new(thrown.clone()))
            })),
            None,
        )
        .unwrap();
    assert_eq!(
        calls.get(),
        0,
        "capturing/downcasting must not project a balance"
    );
    facade.call_method("reset", vec![]).unwrap();
    drop(facade);
    drop(native);
    assert!(owner.upgrade().is_none());
    assert_eq!(new_handle.read().unwrap().total, 3.0);
    for expected_calls in 1..=2 {
        let error = handle.read().unwrap_err();
        assert_eq!(error.phase(), StoreReadPhase::Projection);
        assert!(
            error
                .native_cause()
                .downcast_ref::<Thrown>()
                .unwrap()
                .value
                .same_reference(&cause)
        );
        assert_eq!(calls.get(), expected_calls);
    }
}

#[test]
fn frozen_map_failure_retains_prethrow_row_write_and_skips_notifications() {
    let expected = source();
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    for case in expected["integrity"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["operation"] == "freeze")
    {
        let (_, facade) = fixture();
        let name = case["name"].as_str().unwrap();
        let market = name.starts_with("markets");
        let key = if name.ends_with("BySymbol") {
            "BTCUSD"
        } else if market {
            "m"
        } else {
            "t"
        };
        let map = facade.get(name).unwrap();
        let row = map.get(key).unwrap();
        let notices = Rc::new(Cell::new(0));
        let count = notices.clone();
        facade
            .call_method(
                "subscribe",
                vec![Js::function("notice", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        let target = if case["subject"] == "map" {
            map
        } else {
            row.clone()
        };
        globals
            .get("Object")
            .unwrap()
            .call_method("freeze", vec![target])
            .unwrap();
        let error = facade
            .call_method(
                if market {
                    "updateMarket"
                } else {
                    "updateTicker"
                },
                vec![
                    Js::object(vec![("id", Js::str(if market { "m" } else { "t" }))]),
                    Js::object(vec![(
                        if market { "active" } else { "bid" },
                        if market {
                            Js::Bool(false)
                        } else {
                            Js::Number(7.0)
                        },
                    )]),
                ],
            )
            .unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        let actual = row.get(if market { "active" } else { "bid" }).unwrap();
        assert!(
            actual.strict_equals(&from_json(&case["rowValue"])),
            "{name} {:?}",
            case["subject"]
        );
        assert_eq!(notices.get(), 0);
    }
}
