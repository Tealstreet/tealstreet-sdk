//! Decoder identities and the real constructed socket's order-book producer.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{
    classes, lifecycle,
    orderbooks::{Level, Levels},
};
use safe_cex_exchange::{js_runtime::value::Js, websocket::WsEvent};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderBookOrders};
use serde_json::{Value, json};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

fn bits(row: &OrderBookOrders) -> [u64; 3] {
    [
        row.price.to_bits(),
        row.amount.to_bits(),
        row.total.to_bits(),
    ]
}

#[test]
fn level_decoder_preserves_fields_bits_and_native_cell_identity() {
    let _: PhantomData<fn(Level) -> Level> =
        PhantomData::<fn(Rc<RefCell<OrderBookOrders>>) -> Rc<RefCell<OrderBookOrders>>>;
    for value in [
        json!({"price":-0.0,"amount":17.25,"total":31.5}),
        json!({"price":23.75,"amount":-0.0,"total":-41.125,"ignored":true}),
    ] {
        let decoded: Level = serde_json::from_value(value.clone()).unwrap();
        let mut parser =
            serde_json::Deserializer::from_str("{\"price\":-0.0,\"amount\":17.25,\"total\":31.5}");
        let qualified = <Level as serde::Deserialize>::deserialize(&mut parser).unwrap();
        assert_eq!(
            bits(&qualified.borrow()),
            [
                (-0.0_f64).to_bits(),
                17.25_f64.to_bits(),
                31.5_f64.to_bits()
            ]
        );
        let expected =
            ["price", "amount", "total"].map(|key| value[key].as_f64().unwrap().to_bits());
        assert_eq!(bits(&decoded.borrow()), expected);
        let retained = decoded.clone();
        assert!(Rc::ptr_eq(&decoded, &retained));
        retained.borrow_mut().amount = 53.625;
        assert_eq!(decoded.borrow().amount.to_bits(), 53.625_f64.to_bits());
    }
    for bad in [
        json!(null),
        json!(false),
        json!(7),
        json!("x"),
        json!([]),
        json!([1, 2, 3]),
        json!({}),
        json!({"price":1,"amount":2}),
        json!({"price":"1","amount":2,"total":3}),
    ] {
        assert!(
            serde_json::from_value::<Level>(bad.clone()).is_err(),
            "forbidden level {bad}"
        );
    }
}

#[test]
fn levels_decoder_preserves_order_members_and_collection_identity() {
    let _: PhantomData<fn(Levels) -> Levels> = PhantomData::<
        fn(
            Rc<RefCell<Vec<Rc<RefCell<OrderBookOrders>>>>>,
        ) -> Rc<RefCell<Vec<Rc<RefCell<OrderBookOrders>>>>>,
    >;
    let raw = r#"[{"price":-0.0,"amount":17.25,"total":31.5},{"price":23.75,"amount":-0.0,"total":-41.125}]"#;
    let decoded: Levels = serde_json::from_str(raw).unwrap();
    let mut parser = serde_json::Deserializer::from_str(raw);
    let qualified = <Levels as serde::Deserialize>::deserialize(&mut parser).unwrap();
    let expected = [
        [
            (-0.0_f64).to_bits(),
            17.25_f64.to_bits(),
            31.5_f64.to_bits(),
        ],
        [
            23.75_f64.to_bits(),
            (-0.0_f64).to_bits(),
            (-41.125_f64).to_bits(),
        ],
    ];
    for rows in [&decoded, &qualified] {
        assert_eq!(rows.borrow().len(), 2);
        for (row, expected) in rows.borrow().iter().zip(expected) {
            assert_eq!(bits(&row.borrow()), expected);
        }
    }
    let cloned = decoded.clone();
    assert!(Rc::ptr_eq(&decoded, &cloned));
    let first = decoded.borrow()[0].clone();
    assert!(Rc::ptr_eq(&first, &cloned.borrow()[0]));
    first.borrow_mut().total = 61.25;
    assert_eq!(
        cloned.borrow()[0].borrow().total.to_bits(),
        61.25_f64.to_bits()
    );
    cloned.borrow_mut().remove(1);
    assert_eq!(decoded.borrow().len(), 1);
    assert!(
        serde_json::from_str::<Levels>("[]")
            .unwrap()
            .borrow()
            .is_empty()
    );
    for bad in [
        json!(null),
        json!(false),
        json!(7),
        json!("x"),
        json!({}),
        json!([null]),
        json!([[1, 2, 3]]),
        json!([{"price":1,"amount":2}]),
    ] {
        assert!(
            serde_json::from_value::<Levels>(bad.clone()).is_err(),
            "forbidden levels {bad}"
        );
    }
}

fn source_book(family: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/orderbook-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows: Vec<_> = corpus["books"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["family"] == family
                && row["operation"] == "emit"
                && row["config"] == json!({"noMarket":true})
        })
        .collect();
    assert_eq!(rows.len(), 1, "one unchanged live source emit case");
    assert!(rows[0]["failure"].is_null());
    let callbacks: Vec<_> = rows[0]["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[0] == "callback")
        .collect();
    assert_eq!(
        callbacks.len(),
        2,
        "live source really invoked both callbacks"
    );
    callbacks[0][2].clone()
}

fn emitted_books(name: ExchangeName) -> Vec<Js> {
    let h = harness(name.clone(), options(), |_| {});
    let rt = &h.exchange.rt;
    let socket = if name == ExchangeName::Kraken {
        classes::futures_public(rt, Rc::new(rt.this.clone())).unwrap()
    } else {
        classes::spot_public(rt, Rc::new(rt.this.clone())).unwrap()
    };
    let host = rt.shared.extra.sockets.borrow().last().unwrap().clone();
    let books = Rc::new(RefCell::new(Vec::new()));
    let output = books.clone();
    let symbol = if name == ExchangeName::Kraken {
        "PF_XBTUSD"
    } else {
        "XBT/USD"
    };
    let cleanup = socket
        .call_method(
            "listenOrderBook",
            vec![
                Js::str(symbol),
                Js::function("bookAliasObserver", move |_, args| {
                    output.borrow_mut().push(args[0].clone());
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
    if name == ExchangeName::Kraken {
        lifecycle::futures_public_connect(host.clone()).unwrap();
    } else {
        lifecycle::spot_public_connect(host.clone()).unwrap();
    }
    h.runtime.pump();
    h.runtime.advance(100.0);
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    let physical = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        physical.sent.borrow().iter().any(|wire| {
            let value: Value = serde_json::from_str(wire).unwrap();
            if name == ExchangeName::Kraken {
                value["feed"] == "book" && value["product_ids"] == json!([symbol])
            } else {
                value["params"]["channel"] == "book" && value["params"]["symbol"] == json!([symbol])
            }
        }),
        "real native subscription"
    );
    let bids = json!([{"price":5.5,"qty":7.25},{"price":17.25,"qty":11.5}]);
    let asks = json!([{"price":41.25,"qty":43.5},{"price":37.75,"qty":47.125}]);
    let frame = if name == ExchangeName::Kraken {
        json!({"feed":"book_snapshot","product_id":symbol,"bids":bids,"asks":asks})
    } else {
        json!({"channel":"book","type":"snapshot","data":[{"symbol":symbol,"bids":bids,"asks":asks,"checksum":123}]})
    };
    physical
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
    assert_eq!(books.borrow().len(), 1, "production book callback");
    cleanup.call(Js::Undefined, vec![]).unwrap();
    let result = books.borrow().clone();
    result
}

#[test]
fn native_level_values_reach_constructed_futures_and_spot_callbacks() {
    for (name, family) in [
        (ExchangeName::Kraken, "Futures"),
        (ExchangeName::Krakenspot, "Spot"),
    ] {
        let books = emitted_books(name);
        let source = source_book(family);
        for side in ["bids", "asks"] {
            let levels = books[0].get(side).unwrap().array_values().unwrap();
            let expected = source[side].as_array().unwrap();
            assert_eq!(expected.len(), 2, "nonempty source {side}");
            assert_eq!(levels.len(), expected.len(), "native {side} length");
            for (row, expected) in levels.iter().zip(expected) {
                for field in ["price", "amount", "total"] {
                    let bits =
                        u64::from_str_radix(expected[field]["n"].as_str().unwrap(), 16).unwrap();
                    assert_eq!(
                        row.get(field).unwrap().as_f64().unwrap().to_bits(),
                        bits,
                        "source/native {family} {side} {field}"
                    );
                }
            }
        }
    }
}

#[test]
fn native_levels_collection_keeps_both_sides_and_sort_order() {
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let books = emitted_books(name);
        for (side, expected) in [("bids", [17.25_f64, 5.5]), ("asks", [37.75, 41.25])] {
            let levels = books[0].get(side).unwrap().array_values().unwrap();
            assert_eq!(levels.len(), 2, "native {side} collection length");
            assert_eq!(
                levels
                    .iter()
                    .map(|row| row.get("price").unwrap().as_f64().unwrap().to_bits())
                    .collect::<Vec<_>>(),
                expected.map(f64::to_bits)
            );
        }
    }
}
