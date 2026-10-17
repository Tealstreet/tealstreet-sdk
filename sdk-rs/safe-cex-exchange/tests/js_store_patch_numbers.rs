//! JS patch ingress into the production Store, before any JSON wire encoding.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_json, from_typed},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use serde_json::json;
use std::rc::Rc;

fn seeded() -> (Rc<DefaultStore>, Js) {
    let native = Rc::new(DefaultStore::new());
    let update: StoreUpdate = serde_json::from_value(json!({
        "positions":[{"id":"p","symbol":"BTCUSD","side":"long","entryPrice":20000,
            "notional":1000,"leverage":10,"unrealizedPnl":0,"contracts":0.05,"liquidationPrice":10000}],
        "orders":[{"id":"o","symbol":"BTCUSD","status":"open","type":"limit","side":"buy",
            "price":20000,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":1}],
        "tickers":[{"id":"t","symbol":"BTCUSD","bid":19999,"ask":20001,"last":20000,"mark":20000,
            "index":20000,"percentage":0,"openInterest":1,"fundingRate":0,"fundingTime":1,"volume":1,"quoteVolume":20000}],
        "markets":[{"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
            "precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}}]
    })).unwrap();
    Store::update(native.as_ref(), update, false).unwrap();
    let facade = store::facade(native.clone());
    (native, facade)
}

#[test]
fn update_positions_keeps_native_leverage_bits() {
    let (native, facade) = seeded();
    for value in [
        f64::NAN,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        f64::MAX,
        5e-324,
    ] {
        facade
            .call_method(
                "updatePositions",
                vec![Js::array(vec![Js::array(vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    Js::object(vec![("leverage", Js::Number(value))]),
                ])])],
            )
            .unwrap();
        assert_eq!(
            native.positions()[0].leverage.to_bits(),
            value.to_bits(),
            "updatePositions retains native leverage bits for {value:?}"
        );
    }
}

#[test]
fn all_model_patch_methods_keep_every_numeric_field() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "updatePosition",
            "p",
            &[
                "entryPrice",
                "notional",
                "leverage",
                "unrealizedPnl",
                "realizedPnl",
                "contracts",
                "liquidationPrice",
                "marginSize",
                "lastUpdate",
            ],
        ),
        (
            "updateOrder",
            "o",
            &[
                "price",
                "amount",
                "filled",
                "remaining",
                "lastUpdate",
                "takeProfit",
                "stopLoss",
            ],
        ),
        (
            "updateTicker",
            "t",
            &[
                "bid",
                "ask",
                "last",
                "mark",
                "index",
                "midPrice",
                "percentage",
                "openInterest",
                "fundingRate",
                "fundingTime",
                "volume",
                "quoteVolume",
                "timestamp",
            ],
        ),
        (
            "updateMarket",
            "m",
            &["contractSize", "makerFee", "takerFee"],
        ),
    ];
    for &(method, id, fields) in cases {
        for &field in fields {
            for value in [
                f64::NAN,
                -0.0,
                f64::INFINITY,
                f64::NEG_INFINITY,
                5e-324,
                42.25,
            ] {
                let (native, facade) = seeded();
                facade
                    .call_method(
                        method,
                        vec![
                            Js::object(vec![("id", Js::str(id))]),
                            Js::object(vec![(field, Js::Number(value))]),
                        ],
                    )
                    .unwrap_or_else(|e| {
                        panic!("{method}.{field} refused {value:?}: {}", e.message())
                    });
                let row = match method {
                    "updatePosition" => from_typed(&native.positions()[0]),
                    "updateOrder" => from_typed(&native.orders()[0]),
                    "updateTicker" => from_typed(&native.tickers()[0]),
                    "updateMarket" => from_typed(&native.markets()[0]),
                    _ => unreachable!(),
                }
                .unwrap();
                assert_eq!(
                    row.get(field).unwrap().as_f64().unwrap().to_bits(),
                    value.to_bits(),
                    "{method}.{field} retains {value:?}"
                );
            }
        }
    }
}

#[test]
fn numeric_patch_keeps_omission_null_and_other_field_policy() {
    let (native, facade) = seeded();
    facade
        .call_method(
            "updatePosition",
            vec![
                Js::object(vec![("id", Js::str("p"))]),
                Js::object(vec![
                    ("leverage", Js::Undefined),
                    ("entryPrice", Js::function("ignored", |_, _| Ok(Js::Null))),
                    ("marginType", Js::str("isolated")),
                    ("info", Js::Null),
                    ("unknown", Js::Number(f64::NAN)),
                ]),
            ],
        )
        .unwrap();
    assert_eq!(native.positions()[0].leverage, 10.0);
    assert_eq!(native.positions()[0].entry_price, 20000.0);
    assert_eq!(
        native.positions()[0].margin_type,
        Some(safe_cex_types::MarginType::Isolated)
    );
    assert_eq!(native.positions()[0].info, Some(serde_json::Value::Null));
    assert!(
        facade
            .call_method(
                "updatePosition",
                vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    from_json(&json!({"leverage":null})),
                ]
            )
            .is_err(),
        "explicit null is still refused as a numeric patch"
    );
    assert_eq!(native.positions()[0].leverage, 10.0);
    for (value, expected) in [(42.0, json!(42.0)), (f64::NAN, json!(null))] {
        facade
            .call_method(
                "updatePosition",
                vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    Js::object(vec![("info", Js::Number(value))]),
                ],
            )
            .unwrap();
        assert_eq!(
            native.positions()[0].info,
            Some(expected),
            "opaque info keeps its JSON policy"
        );
    }
    // An absent position remains absent: the numeric fix does not turn update into insertion.
    facade
        .call_method(
            "updatePositions",
            vec![Js::array(vec![Js::array(vec![
                Js::object(vec![("id", Js::str("absent"))]),
                Js::object(vec![("leverage", Js::Number(f64::NAN))]),
            ])])],
        )
        .unwrap();
    assert_eq!(native.positions().len(), 1);
}
